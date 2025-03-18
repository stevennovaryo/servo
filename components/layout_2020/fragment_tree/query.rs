use std::collections::HashMap;
use std::hash::RandomState;

use app_units::Au;
use base::id::PipelineId;
use euclid::Vector2D;
use webrender_api::ExternalScrollId;
use webrender_api::units::LayoutPixel;

use super::{ContainingBlockManager, Fragment, FragmentTree};
use crate::geom::{PhysicalRect, PhysicalVec};
use crate::style_ext::ComputedValuesExt;

// /// Trait of state manager for [Fragment::find], contains several(s) callbacks that will be called
// /// in the iteration and function to share the information.
// pub trait FragmentTreeQueryManager<C, T, P> {
//     /// Get information that is scored in the state manager.
//     fn precompute_state_and_then(&self, parent: &Fragment, predicate: P) -> Option<T>;

//     fn get_payload(&self, fragment: &Fragment) -> &T;
// }

pub struct FragmentTreeQueryContext<'a, T, D: 'a + Clone> {
    manager: ContainingBlockManager<'a, T>,
    level: usize,
    additional_data: D,
}

pub struct ContainingBlockInfoData<'a> {
    pub(crate) scroll_offsets:
        &'a HashMap<ExternalScrollId, Vector2D<f32, LayoutPixel>, RandomState>,
    pub(crate) pipeline_id: PipelineId,
}

impl Clone for ContainingBlockInfoData<'_> {
    fn clone(&self) -> Self {
        Self {
            scroll_offsets: self.scroll_offsets,
            pipeline_id: self.pipeline_id,
        }
    }
}

impl<'a> FragmentTreeQueryContext<'a, ContainingBlockQueryInfo, ContainingBlockInfoData<'a>> {
    fn new_for_next_level(
        &self,
        manager: ContainingBlockManager<'a, ContainingBlockQueryInfo>,
    ) -> Self {
        Self {
            manager,
            level: self.level + 1,
            additional_data: self.additional_data.clone(),
        }
    }
}

pub type ContainingBlockInfoContext<'a> =
    FragmentTreeQueryContext<'a, ContainingBlockQueryInfo, ContainingBlockInfoData<'a>>;

impl ContainingBlockInfoContext<'_> {
    pub(crate) fn for_fragment_tree_and_then<
        P: FnMut(&ContainingBlockInfoContext<'_>) -> Option<ContainingBlockQueryInfo>,
    >(
        fragment_tree: &FragmentTree,
        additional_data: ContainingBlockInfoData,
        mut predicate: P,
    ) -> Option<ContainingBlockQueryInfo> {
        let scroll_offset = additional_data
            .scroll_offsets
            .get(&additional_data.pipeline_id.root_scroll_id())
            .map(|offset| PhysicalVec::new(Au::from_f32_px(offset.x), Au::from_f32_px(offset.y)))
            .unwrap_or_default();

        let initial_containing_block_info = ContainingBlockQueryInfo {
            rect: fragment_tree.initial_containing_block,
            scroll_offset,
        };
        let fixed_containing_block_info = ContainingBlockQueryInfo {
            rect: fragment_tree.initial_containing_block,
            scroll_offset: Vector2D::zero(),
        };

        let info: ContainingBlockManager<'_, ContainingBlockQueryInfo> = ContainingBlockManager {
            for_non_absolute_descendants: &initial_containing_block_info,
            for_absolute_descendants: Some(&initial_containing_block_info),
            for_absolute_and_fixed_descendants: &fixed_containing_block_info,
        };

        let initial_context = FragmentTreeQueryContext {
            manager: info,
            level: 0,
            additional_data,
        };

        predicate(&initial_context)
    }
}

impl<'a> ContainingBlockInfoContext<'a> {
    pub(crate) fn precompute_state_and_then<
        P: FnMut(&ContainingBlockInfoContext<'_>) -> Option<ContainingBlockQueryInfo>,
    >(
        &self,
        parent: &Fragment,
        mut predicate: P,
    ) -> Option<ContainingBlockQueryInfo> {
        let containing_block = self.manager.get_containing_block_for_fragment(parent);
        let additional_data = &self.additional_data;

        match parent {
            Fragment::Box(fragment) | Fragment::Float(fragment) => {
                let fragment = fragment.borrow();
                let scroll_id = fragment.base.tag.map(|tag| {
                    ExternalScrollId(
                        tag.to_display_list_fragment_id(),
                        additional_data.pipeline_id.into(),
                    )
                });
                let scroll_offset = scroll_id
                    .and_then(|id| additional_data.scroll_offsets.get(&id))
                    .map(|offset| {
                        PhysicalVec::new(Au::from_f32_px(offset.x), Au::from_f32_px(offset.y))
                    })
                    .unwrap_or_default();

                let content_rect_info = containing_block
                    .new_relative_transformed_child(fragment.content_rect, scroll_offset);
                let padding_rect_info = containing_block
                    .new_relative_transformed_child(fragment.padding_rect(), scroll_offset);

                let new_manager = if fragment
                    .style
                    .establishes_containing_block_for_all_descendants(fragment.base.flags)
                {
                    self.manager.new_for_absolute_and_fixed_descendants(
                        &content_rect_info,
                        &padding_rect_info,
                    )
                } else if fragment
                    .style
                    .establishes_containing_block_for_absolute_descendants(fragment.base.flags)
                {
                    self.manager
                        .new_for_absolute_descendants(&content_rect_info, &padding_rect_info)
                } else {
                    self.manager
                        .new_for_non_absolute_descendants(&content_rect_info)
                };

                predicate(&self.new_for_next_level(new_manager))
            },
            Fragment::Positioning(fragment) => {
                let fragment = fragment.borrow();
                let scroll_id = fragment.base.tag.map(|tag| {
                    ExternalScrollId(
                        tag.to_display_list_fragment_id(),
                        additional_data.pipeline_id.into(),
                    )
                });
                let scroll_offset = scroll_id
                    .and_then(|id| additional_data.scroll_offsets.get(&id))
                    .map(|offset| {
                        PhysicalVec::new(Au::from_f32_px(offset.x), Au::from_f32_px(offset.y))
                    })
                    .unwrap_or_default();

                let content_rect_info =
                    containing_block.new_relative_transformed_child(fragment.rect, scroll_offset);

                let new_manager = self
                    .manager
                    .new_for_non_absolute_descendants(&content_rect_info);

                // predicate(&self.new_for_next_level(new_manager));
                predicate(&self.new_for_next_level(new_manager))
            },
            _ => None,
        }
    }

    pub(crate) fn get_payload(&self, fragment: &Fragment) -> &ContainingBlockQueryInfo {
        self.manager.get_containing_block_for_fragment(fragment)
    }
}

/// Containing block rect with additional information required for a query.
pub(crate) struct ContainingBlockQueryInfo {
    /// Containing block rect, that bounds the children.
    pub(crate) rect: PhysicalRect<Au>,

    /// The scroll offset of the containing block has.
    pub(crate) scroll_offset: PhysicalVec<Au>,
}

impl ContainingBlockQueryInfo {
    /// Transform child's rectangle according to this containing block transformation.
    /// TODO: this is supposed to handle CSS transform but it is not happening.
    pub(crate) fn transform_rect_relative_to_self(
        &self,
        rect: PhysicalRect<Au>,
    ) -> PhysicalRect<Au> {
        rect.translate(self.rect.origin.to_vector() + self.scroll_offset)
    }

    /// New containing block that is a child of this containing block with
    /// ancestor's transformation applied.
    pub(crate) fn new_relative_transformed_child(
        &self,
        rect: PhysicalRect<Au>,
        scroll_offset: PhysicalVec<Au>,
    ) -> ContainingBlockQueryInfo {
        ContainingBlockQueryInfo {
            rect: self.transform_rect_relative_to_self(rect),
            scroll_offset,
        }
    }
}
