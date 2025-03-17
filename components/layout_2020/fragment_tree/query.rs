use std::collections::HashMap;
use std::hash::RandomState;
use std::rc::Rc;

use app_units::Au;
use base::id::PipelineId;
use euclid::Vector2D;
use webrender_api::ExternalScrollId;
use webrender_api::units::LayoutPixel;

use super::{ContainingBlockManagerRc, ContainingBlockQueryInfo, Fragment, FragmentTree};
use crate::geom::PhysicalVec;
use crate::style_ext::ComputedValuesExt;

/// Trait of state manager for [Fragment::find], contains several(s) callbacks that will be called
/// in the iteration and function to share the information.
pub trait FragmentTreeQueryManager<T> {
    /// Get information that is scored in the state manager.
    fn get_payload(&self, fragment: &Fragment) -> Rc<T>;

    /// Callback called before entering any node. If this function return false, then we would not
    /// traverse the respective node.
    fn before_entering_node(&mut self, parent: &Fragment, node: &Fragment) -> bool;

    /// Callback called on exiting any node.
    fn on_exiting_node(&mut self, node: &Fragment);
}

pub struct FragmentTreeQueryContext<'a, T> {
    manager_stacks: Vec<Box<dyn ContainingBlockManagerRcStack<T>>>,
    level: usize,
    scroll_offsets: &'a HashMap<ExternalScrollId, Vector2D<f32, LayoutPixel>, RandomState>,
    pipeline_id: PipelineId,
}

pub trait ContainingBlockManagerRcStack<T: ?Sized> {
    fn get_containing_block_for_fragment(&self, fragment: &Fragment) -> Rc<T>;
    fn manager(&self) -> &ContainingBlockManagerRc<T>;
}

pub struct AbsoluteOrFixedPositionedStack<T: ?Sized> {
    manager: ContainingBlockManagerRc<T>,
    content_rect: Rc<T>,
    padding_rect: Rc<T>,
}

impl<T> ContainingBlockManagerRcStack<T> for AbsoluteOrFixedPositionedStack<T> {
    fn get_containing_block_for_fragment(&self, fragment: &Fragment) -> Rc<T> {
        self.manager.get_containing_block_for_fragment(fragment)
    }

    fn manager(&self) -> &ContainingBlockManagerRc<T> {
        &self.manager
    }
}

impl<T> AbsoluteOrFixedPositionedStack<T> {
    fn new_for_absolute_and_fixed_descendants(
        manager: &ContainingBlockManagerRc<T>,
        content_rect: T,
        padding_rect: T,
    ) -> Self {
        let content_rect = Rc::new(content_rect);
        let padding_rect = Rc::new(padding_rect);

        AbsoluteOrFixedPositionedStack {
            manager: manager.new_for_absolute_and_fixed_descendants(
                Rc::clone(&content_rect),
                Rc::clone(&padding_rect),
            ),
            content_rect,
            padding_rect,
        }
    }

    fn new_for_absolute_descendants(
        manager: &ContainingBlockManagerRc<T>,
        content_rect: T,
        padding_rect: T,
    ) -> Self {
        let content_rect = Rc::new(content_rect);
        let padding_rect = Rc::new(padding_rect);

        AbsoluteOrFixedPositionedStack {
            manager: manager.new_for_absolute_descendants(
                Rc::clone(&content_rect),
                Rc::clone(&padding_rect),
            ),
            content_rect,
            padding_rect,
        }
    }
}

struct NonAbsolutePositionedStack<T: ?Sized> {
    manager: ContainingBlockManagerRc<T>,
    content_rect: Rc<T>,
}

impl<T> NonAbsolutePositionedStack<T> {
    fn new_for_non_absolute_descendants(
        manager: &ContainingBlockManagerRc<T>,
        content_rect: T,
    ) -> Self {
        let content_rect = Rc::new(content_rect);
        NonAbsolutePositionedStack {
            manager: manager.new_for_non_absolute_descendants(Rc::clone(&content_rect)),
            content_rect,
        }
    }
}

impl<T> ContainingBlockManagerRcStack<T> for NonAbsolutePositionedStack<T> {
    fn get_containing_block_for_fragment(&self, fragment: &Fragment) -> Rc<T> {
        self.manager.get_containing_block_for_fragment(fragment)
    }

    fn manager(&self) -> &ContainingBlockManagerRc<T> {
        &self.manager
    }
}

pub trait ComputeManagerStack<T> {
    fn new_manager_for_fragment(
        &self,
        containing_block: &T,
        child: &Fragment,
        manager: &ContainingBlockManagerRc<T>,
    ) -> Option<Box<dyn ContainingBlockManagerRcStack<T>>>;
}

impl<T> FragmentTreeQueryContext<'_, T> {
    fn enter_level_with_manager(&mut self, manager: Box<dyn ContainingBlockManagerRcStack<T>>) {
        self.level += 1;
        assert!(self.manager_stacks.len() >= self.level);
        if self.manager_stacks.len() == self.level {
            self.manager_stacks.push(manager);
        } else {
            self.manager_stacks[self.level] = manager;
        }
    }
}

impl<T> FragmentTreeQueryManager<T> for FragmentTreeQueryContext<'_, T>
where
    for<'a> FragmentTreeQueryContext<'a, T>: ComputeManagerStack<T>,
{
    fn get_payload(&self, fragment: &Fragment) -> Rc<T> {
        self.manager_stacks[self.level].get_containing_block_for_fragment(fragment)
    }

    /// For this case we would compute the containing block of the children. To be passed into process func.
    fn before_entering_node(&mut self, parent: &Fragment, node: &Fragment) -> bool {
        let containing_block = self.get_payload(parent);
        let manager = self.manager_stacks[self.level].manager();

        if let Some(new_manager) = self.new_manager_for_fragment(&containing_block, node, manager) {
            self.enter_level_with_manager(new_manager);
            true
        } else {
            false
        }
    }

    fn on_exiting_node(&mut self, _node: &Fragment) {
        // FIXME: This if shouldn't be needed
        if self.level > 0 {
            self.level -= 1;
        }
    }
}

impl ContainingBlockManagerRcStack<ContainingBlockQueryInfo> for ContainingBlockManagerRc<ContainingBlockQueryInfo> {
    fn get_containing_block_for_fragment(&self, fragment: &Fragment) -> Rc<ContainingBlockQueryInfo> {
        self.get_containing_block_for_fragment(fragment)
    }

    fn manager(&self) -> &ContainingBlockManagerRc<ContainingBlockQueryInfo> {
        self
    }
}

impl<'a> FragmentTreeQueryContext<'a, ContainingBlockQueryInfo> {

    pub(crate) fn new_for_fragment_tree(
        fragment_tree: &FragmentTree,
        scroll_offsets: &'a HashMap<ExternalScrollId, Vector2D<f32, LayoutPixel>, RandomState>,
        pipeline_id: PipelineId,
    ) -> Self {
        let scroll_offset = scroll_offsets
            .get(&pipeline_id.root_scroll_id())
            .map(|offset| PhysicalVec::new(Au::from_f32_px(offset.x), Au::from_f32_px(offset.y)))
            .unwrap_or_default();

        let initial_containing_block_info = Rc::new(ContainingBlockQueryInfo {
            rect: fragment_tree.initial_containing_block,
            scroll_offset,
        });
        let fixed_containing_block_info = Rc::new(ContainingBlockQueryInfo {
            rect: fragment_tree.initial_containing_block,
            scroll_offset: Vector2D::zero(),
        });

        let info = ContainingBlockManagerRc {
            for_non_absolute_descendants: Rc::clone(&initial_containing_block_info),
            for_absolute_descendants: Some(Rc::clone(&initial_containing_block_info)),
            for_absolute_and_fixed_descendants: Rc::clone(&fixed_containing_block_info),
        };

        Self {
            manager_stacks: vec![Box::new(info)],
            level: 0,
            scroll_offsets,
            pipeline_id,
        }
    }
}

impl ComputeManagerStack<ContainingBlockQueryInfo>
    for FragmentTreeQueryContext<'_, ContainingBlockQueryInfo>
{
    fn new_manager_for_fragment(
        &self,
        containing_block: &ContainingBlockQueryInfo,
        child: &Fragment,
        manager: &ContainingBlockManagerRc<ContainingBlockQueryInfo>,
    ) -> Option<Box<dyn ContainingBlockManagerRcStack<ContainingBlockQueryInfo>>> {
        match child {
            Fragment::Box(fragment) | Fragment::Float(fragment) => {
                let fragment = fragment.borrow();
                let scroll_id = fragment.base.tag.map(|tag| {
                    ExternalScrollId(tag.to_display_list_fragment_id(), self.pipeline_id.into())
                });
                let scroll_offset = scroll_id
                    .and_then(|id| self.scroll_offsets.get(&id))
                    .map(|offset| {
                        PhysicalVec::new(Au::from_f32_px(offset.x), Au::from_f32_px(offset.y))
                    })
                    .unwrap_or_default();

                let content_rect_info = containing_block
                    .new_relative_transformed_child(fragment.content_rect, scroll_offset);
                let padding_rect_info = containing_block
                    .new_relative_transformed_child(fragment.padding_rect(), scroll_offset);

                let new_manager: Box<dyn ContainingBlockManagerRcStack<ContainingBlockQueryInfo>> =
                    if fragment
                        .style
                        .establishes_containing_block_for_all_descendants(fragment.base.flags)
                    {
                        Box::new(
                            AbsoluteOrFixedPositionedStack::new_for_absolute_and_fixed_descendants(
                                manager,
                                content_rect_info,
                                padding_rect_info,
                            ),
                        )
                    } else if fragment
                        .style
                        .establishes_containing_block_for_absolute_descendants(fragment.base.flags)
                    {
                        Box::new(
                            AbsoluteOrFixedPositionedStack::new_for_absolute_descendants(
                                manager,
                                content_rect_info,
                                padding_rect_info,
                            ),
                        )
                    } else {
                        Box::new(
                            NonAbsolutePositionedStack::new_for_non_absolute_descendants(
                                manager,
                                content_rect_info,
                            ),
                        )
                    };

                Some(new_manager)
            },
            Fragment::Positioning(fragment) => {
                let fragment = fragment.borrow();
                let scroll_id = fragment.base.tag.map(|tag| {
                    ExternalScrollId(tag.to_display_list_fragment_id(), self.pipeline_id.into())
                });
                let scroll_offset = scroll_id
                    .and_then(|id| self.scroll_offsets.get(&id))
                    .map(|offset| {
                        PhysicalVec::new(Au::from_f32_px(offset.x), Au::from_f32_px(offset.y))
                    })
                    .unwrap_or_default();

                let content_rect_info =
                    containing_block.new_relative_transformed_child(fragment.rect, scroll_offset);

                Some(Box::new(
                    NonAbsolutePositionedStack::new_for_non_absolute_descendants(
                        manager,
                        content_rect_info,
                    ),
                ))
            },
            _ => None,
        }
    }
}
