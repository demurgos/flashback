use crate::avm1;
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

#[derive(Debug)]
pub struct Object<'a> {
    pub character: CharacterId,
    pub matrix: swf::Matrix,
    pub name: Option<&'a str>,
}

impl<'a> Clone for Object<'a> {
    fn clone(&self) -> Self {
        Object {
            character: self.character,
            matrix: copy_matrix(&self.matrix),
            name: self.name,
        }
    }
}

impl<'a> Object<'a> {
    pub fn new(character: CharacterId) -> Self {
        Object {
            character,
            matrix: default_matrix(),
            name: None,
        }
    }
}

#[derive(Default, Debug)]
pub struct Layer<'a> {
    pub frames: BTreeMap<Frame, Option<Object<'a>>>,
}

#[derive(Default, Debug)]
pub struct Timeline<'a> {
    pub layers: BTreeMap<Depth, Layer<'a>>,
    pub actions: BTreeMap<Frame, Vec<avm1::Code<'a>>>,
    pub frame_count: Frame,
}

#[derive(Default)]
pub struct TimelineBuilder<'a> {
    timeline: Timeline<'a>,
    current_frame: Frame,
}

impl<'a> TimelineBuilder<'a> {
    pub fn place_object(&mut self, place: &'a swf::tags::PlaceObject) {
        let layer = self.timeline.layers.entry(Depth(place.depth)).or_default();

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
                        .expect("TimelineBuilder::place_object: missing `character_id`"),
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
        if let Some(name) = &place.name {
            obj.name = Some(name);
        }
    }

    pub fn remove_object(&mut self, remove: &swf::tags::RemoveObject) {
        self.timeline
            .layers
            .get_mut(&Depth(remove.depth))
            .unwrap()
            .frames
            .insert(self.current_frame, None);
    }

    pub fn do_action(&mut self, do_action: &'a swf::tags::DoAction) {
        self.timeline
            .actions
            .entry(self.current_frame)
            .or_default()
            .push(avm1::Code::compile(&do_action.actions))
    }

    pub fn advance_frame(&mut self) {
        self.current_frame = self.current_frame + Frame(1);
    }

    pub fn finish(mut self, frame_count: Frame) -> Timeline<'a> {
        // HACK(eddyb) this should be an error but it happens during testing.
        if self.current_frame != frame_count {
            eprintln!(
                "TimelineBuilder::finish: expected {} frames, found {}",
                frame_count.0, self.current_frame.0,
            );
        }
        self.timeline.frame_count = frame_count;

        self.timeline
    }
}
