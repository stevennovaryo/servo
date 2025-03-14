use std::{cell::{Cell, Ref, RefCell}, rc::Rc};

use app_units::Au;

use crate::geom::PhysicalRect;
use crate::style_ext::ComputedValuesExt;

use super::{ContainingBlockManagerRef, Fragment};

pub type ContainingBlockRect = PhysicalRect<Au>;

pub trait FragmentTreeQueryManager<'a, T> {
    fn enter_level_with_manager(&mut self, manager: ContainingBlockManagerRef<'a, T>);
    fn get_containing_block(&self, fragment: &Fragment) -> Ref<'a, ContainingBlockRect>;
    fn before_entering_child(&mut self, parent: &Fragment, child: &Fragment);
}

pub struct FragmentTreeQueryContext<'a, T> {
    cb_manager_stacks: Vec<ContainingBlockManagerRefStack<'a, T>>,
    level: usize,
}

pub enum ContainingBlockManagerRefStack<'a, T> {
    BoxFragment(BoxFragmentManagerStack<'a, T>),
    PositioningFragment(PositioningFragmentManagerStack<'a, T>),
}

pub struct BoxFragmentManagerStack<'a, T> {
    manager: ContainingBlockManagerRef<'a, T>,
    content_rect: RefCell<T>,
    padding_rect: RefCell<T>,
}

impl<'a, T> BoxFragmentManagerStack<'a, T> {
    fn new(manager: ContainingBlockManagerRef<'a, T>, content_rect: T, padding_rect: T) -> Self {
        // let content_rect = RefCell::new(content_rect);
        // let padding_rect = RefCell::new(padding_rect);
        let new_manager = BoxFragmentManagerStack {
            manager: manager.new_for_absolute_and_fixed_descendants(content_rect, padding_rect),
            content_rect,
            padding_rect,
        };
        new_manager
    }
}

struct PositioningFragmentManagerStack<'a, T> {
    manager: ContainingBlockManagerRef<'a, T>,
    content_rect: T,
}

impl<'a> ContainingBlockManagerRefStack<'a, ContainingBlockRect> {
    fn new_manager_for_fragment(containing_block: ContainingBlockRect, child: &'a Fragment, manager: ContainingBlockManagerRef<'a, ContainingBlockRect>) -> Option<Self> {
        match child {
            Fragment::Box(fragment) | Fragment::Float(fragment) => {
                let fragment = fragment.borrow();
                let content_rect = RefCell::new(fragment
                    .content_rect
                    .translate(containing_block.origin.to_vector()));
                let padding_rect = RefCell::new(fragment
                    .padding_rect()
                    .translate(containing_block.origin.to_vector()));

                // let new_manager = if fragment
                //     .style
                //     .establishes_containing_block_for_all_descendants(fragment.base.flags)
                // {
                //     manager.new_for_absolute_and_fixed_descendants(&content_rect.borrow(), &padding_rect.borrow())
                // } else if fragment
                //     .style
                //     .establishes_containing_block_for_absolute_descendants(fragment.base.flags)
                // {
                //     manager.new_for_absolute_descendants(&content_rect.borrow(), &padding_rect.borrow())
                // } else {
                //     manager.new_for_non_absolute_descendants(&content_rect.borrow())
                // };

                Some(ContainingBlockManagerRefStack::BoxFragment(BoxFragmentManagerStack::new(manager, content_rect.borrow(), padding_rect.borrow())))
            },
            Fragment::Positioning(fragment) => {
                let fragment = fragment.borrow();
                let content_rect = fragment.rect.translate(containing_block.origin.to_vector());
                let new_manager = manager.new_for_non_absolute_descendants(&content_rect);

                Some(ContainingBlockManagerRefStack::PositioningFragment(PositioningFragmentManagerStack {
                    manager: new_manager,
                    content_rect,
                }))
            },
            _ => None,
        }
    }
}

impl<'a> FragmentTreeQueryManager<'a, ContainingBlockRect> for FragmentTreeQueryContext<'a, ContainingBlockRect> {
    fn get_containing_block(&self, fragment: &Fragment) -> Ref<'a, ContainingBlockRect> {
        self.cb_manager_stacks[self.level].get_containing_block_for_fragment(fragment)
    }

    fn enter_level_with_manager(&mut self, manager: ContainingBlockManagerRefStack<'a, ContainingBlockRect>) {
        self.level += 1;
        assert!(self.cb_manager_stacks.len() >= self.level);
        if self.cb_manager_stacks.len() == self.level {
            self.cb_manager_stacks.push(manager);
        } else {
            self.cb_manager_stacks[self.level] = manager;
        }
    }

    ///
    fn before_entering_child(&mut self, parent: &Fragment, child: &Fragment) -> bool {
        let containing_block = self.get_containing_block(parent);
        let manager = self.cb_manager_stacks[self.level];

        if let Some(new_manager) = ContainingBlockManagerRefStack::new_manager_for_fragment(*containing_block, child, manager) {
            self.enter_level_with_manager(new_manager);
            true
        } else {
            false
        }
    }
}