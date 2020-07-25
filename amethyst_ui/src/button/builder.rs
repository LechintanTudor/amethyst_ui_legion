use crate::{Anchor, FontAsset, Stretch, UiImage};
use amethyst_assets::Handle;
use amethyst_core::{
    ecs::{
        prelude::*,
        storage::Component,
    },
};
use amethyst_rendy::palette::Srgba;

const DEFAULT_Z: f32 = 1.0;
const DEFAULT_WIDTH: f32 = 128.0;
const DEFAULT_HEIGHT: f32 = 64.0;
const DEFAULT_FONT_SIZE: f32 = 32.0;
const DEFAULT_TEXT_COLOR: (f32, f32, f32, f32) = (0.0, 0.0, 0.0, 1.0);
const DEFAULT_BACKGROUND_COLOR: (f32, f32, f32, f32) = (0.8, 0.8, 0.8, 1.0);

pub trait UiButtonBuilderTarget {
    fn create_entity(&mut self) -> Entity;

    fn add_component<C>(&mut self, entity: Entity, component: C)
    where
        C: Component;
}

impl UiButtonBuilderTarget for World {
    fn create_entity(&mut self) -> Entity {
        self.insert((), Some(()))[0]
    }

    fn add_component<C>(&mut self, entity: Entity, component: C)
    where
        C: Component
    {
        self.add_component(entity, component).unwrap();
    }
}

impl UiButtonBuilderTarget for CommandBuffer {
    fn create_entity(&mut self) -> Entity {
        self.insert((), Some(()))[0]
    }

    fn add_component<C>(&mut self, entity: Entity, component: C)
    where
        C: Component
    {
        self.add_component(entity, component);
    }
}

#[derive(Clone, Debug)]
pub struct UiButtonBuilder {
    x: f32,
    y: f32,
    z: f32,
    width: f32,
    height: f32,
    anchor: Anchor,
    pivot: Anchor,
    stretch: Stretch,
    text: String,
    text_color: Srgba,
    font: Option<Handle<FontAsset>>,
    font_size: f32,
    image: Option<UiImage>,
    parent: Option<Entity>,
}

impl Default for UiButtonBuilder {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: DEFAULT_Z,
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            anchor: Anchor::Middle,
            pivot: Anchor::Middle,
            stretch: Stretch::NoStretch,
            text: String::new(),
            text_color: Srgba::from_components(DEFAULT_TEXT_COLOR),
            font: None,
            font_size: DEFAULT_FONT_SIZE,
            image: Some(UiImage::SolidColor(Srgba::from_components(DEFAULT_BACKGROUND_COLOR))),
            parent: None,
        }
    }
}

impl UiButtonBuilder {
    pub fn with_position(mut self, x: f32, y: f32) -> Self {
        self.x = x;
        self.y = y;
        self
    }

    pub fn with_layer(mut self, z: f32) -> Self {
        self.z = z;
        self
    }

    pub fn with_size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn with_anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    pub fn with_pivot(mut self, pivot: Anchor) -> Self {
        self.pivot = pivot;
        self
    }

    pub fn with_stretch(mut self, stretch: Stretch) -> Self {
        self.stretch = stretch;
        self
    }

    pub fn with_text<S>(mut self, text: S) -> Self
    where
        S: ToString
    {
        self.text = text.to_string();
        self
    }

    pub fn with_text_color(mut self, text_color: Srgba) -> Self {
        self.text_color = text_color;
        self
    }

    pub fn with_font(mut self, font: Handle<FontAsset>) -> Self {
        self.font = Some(font);
        self
    }

    pub fn with_font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self
    }

    pub fn with_image(mut self, image: UiImage) -> Self {
        self.image = Some(image);
        self
    }

    pub fn with_parent(mut self, parent: Entity) -> Self {
        self.parent = Some(parent);
        self
    }

    pub fn build<T>(self, target: T) -> Entity
    where
        T: UiButtonBuilderTarget
    {
        todo!()
    }
}