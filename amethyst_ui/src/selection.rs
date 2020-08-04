use crate::{UiEvent, UiEventType};
use amethyst_core::{ecs::prelude::*, shrev::EventChannel};
use amethyst_input::{BindingTypes, InputHandler};
use std::collections::HashSet;
use winit::VirtualKeyCode;

/// Keeps track of selected entities.
#[derive(Clone, Default, Debug)]
pub struct SelectedEntities {
    entities: HashSet<Entity>,
    last: Option<Entity>,
}

impl SelectedEntities {
    /// Unselect all entities.
    pub fn clear(&mut self) {
        self.entities.clear();
        self.last = None;
    }

    /// Mark a new entity as selected.
    pub fn insert(&mut self, entity: Entity) {
        self.entities.insert(entity);
        self.last = Some(entity);
    }

    /// Unselect an entity.
    pub fn remove(&mut self, entity: Entity) {
        self.entities.remove(&entity);

        if self.last == Some(entity) {
            self.last = self.entities.iter().next().cloned();
        }
    }

    /// Checks if entity is selected.
    pub fn contains(&self, entity: Entity) -> bool {
        self.entities.contains(&entity)
    }

    /// Returns all selected entities.
    pub fn entities(&self) -> &HashSet<Entity> {
        &self.entities
    }

    /// Returns the last selected entity (if any).
    pub fn last(&self) -> Option<Entity> {
        self.last
    }
}

/// Enables mouse selection when attached to a UI element.
/// * `G` represents the selection group of the entity.
/// Entitites can be selected together only if they belong
/// to the same selection group.
#[derive(Copy, Clone, Debug)]
pub struct Selectable<G>
where
    G: Send + Sync + PartialEq + 'static,
{
    /// The order in which entities are selected
    pub order: u32,
    /// The selection group to which the entity belongs
    pub multi_select_group: Option<G>,
    /// Whether this UI element can be selected together with other
    /// elements without needing to press the shift or control keys
    pub auto_multi_select: bool,
    /// Whether to ignore inputs when this element is selected
    pub consumes_inputs: bool,
}

pub(crate) fn build_mouse_selection_system<T, G>(
    _world: &mut World,
    resources: &mut Resources,
) -> Box<dyn Schedulable>
where
    T: BindingTypes,
    G: Send + Sync + PartialEq + 'static,
{
    let mut ui_event_reader = resources
        .get_mut_or_default::<EventChannel<UiEvent>>()
        .unwrap()
        .register_reader();

    let mut emitted_ui_events = Vec::<UiEvent>::new();

    SystemBuilder::<()>::new("MouseSelectionSystem")
        .read_resource::<InputHandler<T>>()
        .write_resource::<EventChannel<UiEvent>>()
        .write_resource::<SelectedEntities>()
        .read_component::<Selectable<G>>()
        .build(move |_, world, resources, _| {
            let (input, ui_events, selected) = resources;
            let ctrl = input.key_is_down(VirtualKeyCode::LControl)
                | input.key_is_down(VirtualKeyCode::RControl);

            for event in ui_events.read(&mut ui_event_reader) {
                if event.event_type == UiEventType::ClickStart {
                    let entity = event.target;

                    let selectable = match world.get_component::<Selectable<G>>(entity) {
                        Some(selectable) => selectable,
                        None => {
                            emitted_ui_events.extend(
                                selected
                                    .entities
                                    .drain()
                                    .map(|e| UiEvent::new(UiEventType::Blur, e)),
                            );
                            continue;
                        }
                    };

                    let same_select_group = {
                        if let Some(last_entity) = selected.last() {
                            if let Some(last_selectable) =
                                world.get_component::<Selectable<G>>(last_entity)
                            {
                                last_selectable.multi_select_group == selectable.multi_select_group
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    };

                    if same_select_group && (ctrl || selectable.auto_multi_select) {
                        selected.insert(entity);
                        emitted_ui_events.push(UiEvent::new(UiEventType::Focus, entity));
                    } else {
                        for &entity in selected.entities() {
                            emitted_ui_events.push(UiEvent::new(UiEventType::Blur, entity));
                        }

                        selected.clear();
                        selected.insert(entity);

                        emitted_ui_events.push(UiEvent::new(UiEventType::Focus, entity));
                    }
                }
            }

            ui_events.iter_write(emitted_ui_events.drain(..));
        })
}
