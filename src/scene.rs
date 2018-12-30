use crate::dictionary::CharacterId;
use std::collections::BTreeMap;
use std::ops::Add;
use swf_tree as swf;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Depth(pub u16);

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Frame(pub u16);

impl Add for Frame {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Frame(self.0 + other.0)
    }
}

// FIXME(eddyb) upstream these as methods on `swf-fixed` types.
fn sfixed16p16_epsilons(x: &swf::fixed_point::Sfixed16P16) -> i32 {
    unsafe { std::mem::transmute_copy(x) }
}

// FIXME(eddyb) upstream these as `#[derive(Copy, Clone)]`.
fn copy_sfixed16p16(x: &swf::fixed_point::Sfixed16P16) -> swf::fixed_point::Sfixed16P16 {
    swf::fixed_point::Sfixed16P16::from_epsilons(sfixed16p16_epsilons(x))
}

fn copy_matrix(matrix: &swf::Matrix) -> swf::Matrix {
    swf::Matrix {
        scale_x: copy_sfixed16p16(&matrix.scale_x),
        scale_y: copy_sfixed16p16(&matrix.scale_y),
        rotate_skew0: copy_sfixed16p16(&matrix.rotate_skew0),
        rotate_skew1: copy_sfixed16p16(&matrix.rotate_skew1),
        translate_x: matrix.translate_x,
        translate_y: matrix.translate_y,
    }
}

fn default_matrix() -> swf::Matrix {
    swf::Matrix {
        scale_x: swf::fixed_point::Sfixed16P16::from_epsilons(1 << 16),
        scale_y: swf::fixed_point::Sfixed16P16::from_epsilons(1 << 16),
        rotate_skew0: swf::fixed_point::Sfixed16P16::from_epsilons(0),
        rotate_skew1: swf::fixed_point::Sfixed16P16::from_epsilons(0),
        translate_x: 0,
        translate_y: 0,
    }
}

pub struct Object {
    pub character: CharacterId,
    pub matrix: swf::Matrix,
}

impl Object {
    pub fn new(character: CharacterId) -> Self {
        Object {
            character,
            matrix: default_matrix(),
        }
    }
}

impl Clone for Object {
    fn clone(&self) -> Self {
        Object {
            character: self.character,
            matrix: copy_matrix(&self.matrix),
        }
    }
}

#[derive(Default)]
pub struct Layer {
    pub frames: BTreeMap<Frame, Option<Object>>,
}

impl Layer {
    pub fn any_changes_in_frame(&self, frame: Frame) -> bool {
        self.frames.contains_key(&frame)
    }

    pub fn object_at_frame(&self, frame: Frame) -> Option<&Object> {
        // Find the last frame with a change to this object.
        self.frames
            .range(..=frame)
            .rev()
            .next()
            .and_then(|(_, obj)| obj.as_ref())
    }
}

#[derive(Default)]
pub struct Scene {
    pub layers: BTreeMap<Depth, Layer>,
}

#[derive(Default)]
pub struct SceneBuilder {
    scene: Scene,
    current_frame: Frame,
}

impl SceneBuilder {
    pub fn place_object(&mut self, place: &swf::tags::PlaceObject) {
        let layer = self.scene.layers.entry(Depth(place.depth)).or_default();

        // Find the last changed frame for this object, if it's not
        // the current one, and copy its state of the object.
        let prev_obj = match layer.frames.range(..=self.current_frame).rev().next() {
            Some((&frame, obj)) if frame != self.current_frame => obj.clone(),
            _ => None,
        };

        let obj = layer
            .frames
            .entry(self.current_frame)
            .or_insert(prev_obj)
            .get_or_insert_with(|| {
                Object::new(
                    place
                        .character_id
                        .map(CharacterId)
                        .expect("SceneBuilder::place_object: missing `character_id`"),
                )
            });

        if let Some(character) = place.character_id.map(CharacterId) {
            if place.is_move {
                *obj = Object::new(character);
            } else {
                assert_eq!(obj.character, character);
            }
        }
        if let Some(matrix) = &place.matrix {
            obj.matrix = copy_matrix(matrix);
        }
    }

    pub fn remove_object(&mut self, remove: &swf::tags::RemoveObject) {
        self.scene
            .layers
            .get_mut(&Depth(remove.depth))
            .expect("SceneBuilder::remove_object: no object at depth level")
            .frames
            .insert(self.current_frame, None);
    }

    pub fn advance_frame(&mut self) {
        self.current_frame = self.current_frame + Frame(1);
    }

    pub fn finish(self, movie: &swf::Movie) -> Scene {
        assert_eq!(self.current_frame, Frame(movie.header.frame_count));

        self.scene
    }
}
