use std::io;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

/// A single stereo audio sample in 32-bit float PCM format.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {

    /// The current amplitude on the left channel. Usually on a scale from -1
    /// to 1.
    pub left: f32,

    /// The current amplitude on the right channel. Usually on a scale from -1
    /// to 1.
    pub right: f32
}

impl Sample {

    pub const ZERO: Sample = Sample {
        left: 0.0,
        right: 0.0
    };
}

impl AddAssign for Sample {
    fn add_assign(&mut self, rhs: Sample) {
        self.left += rhs.left;
        self.right += rhs.right;
    }
}

impl AddAssign<&Sample> for Sample {
    fn add_assign(&mut self, rhs: &Sample) {
        self.left += rhs.left;
        self.right += rhs.right;
    }
}

impl Add for Sample {
    type Output = Sample;

    fn add(mut self, rhs: Sample) -> Sample {
        self += rhs;
        self
    }
}

impl Add<&Sample> for Sample {
    type Output = Sample;

    fn add(mut self, rhs: &Sample) -> Sample {
        self += rhs;
        self
    }
}

impl Add for &Sample {
    type Output = Sample;

    fn add(self, rhs: &Sample) -> Sample {
        self.clone() + rhs
    }
}

impl SubAssign for Sample {
    fn sub_assign(&mut self, rhs: Sample) {
        self.left -= rhs.left;
        self.right -= rhs.right;
    }
}

impl SubAssign<&Sample> for Sample {
    fn sub_assign(&mut self, rhs: &Sample) {
        self.left -= rhs.left;
        self.right -= rhs.right;
    }
}

impl Sub for Sample {
    type Output = Sample;

    fn sub(mut self, rhs: Sample) -> Sample {
        self -= rhs;
        self
    }
}

impl Sub<&Sample> for Sample {
    type Output = Sample;

    fn sub(mut self, rhs: &Sample) -> Sample {
        self -= rhs;
        self
    }
}

impl Sub for &Sample {
    type Output = Sample;

    fn sub(self, rhs: &Sample) -> Sample {
        self.clone() - rhs
    }
}

impl MulAssign<f32> for Sample {
    fn mul_assign(&mut self, rhs: f32) {
        self.left *= rhs;
        self.right *= rhs;
    }
}

impl Mul<f32> for Sample {
    type Output = Sample;

    fn mul(mut self, rhs: f32) -> Sample {
        self *= rhs;
        self
    }
}

impl Mul<f32> for &Sample {
    type Output = Sample;

    fn mul(self, rhs: f32) -> Sample {
        self.clone() * rhs
    }
}

impl DivAssign<f32> for Sample {
    fn div_assign(&mut self, rhs: f32) {
        self.left /= rhs;
        self.right /= rhs;
    }
}

impl Div<f32> for Sample {
    type Output = Sample;

    fn div(mut self, rhs: f32) -> Sample {
        self /= rhs;
        self
    }
}

impl Div<f32> for &Sample {
    type Output = Sample;

    fn div(self, rhs: f32) -> Sample {
        self.clone() / rhs
    }
}

/// A trait for types which can read audio data in the form of [Sample]s. The
/// interface is similar to that of the IO [Read](std::io::Read) trait.
pub trait AudioSource {
    fn read(&mut self, buf: &mut [Sample]) -> Result<usize, io::Error>;

    fn has_child(&self) -> bool
    where
        Self: Sized
    {
        false
    }

    fn into_child(self) -> Box<dyn AudioSource>
    where
        Self: Sized
    {
        panic!("audio source has no child")
    }
}

/// A trait for types which can offer a list or enumeration of [AudioSource]s,
/// such as a playlist or loop functionality.
pub trait AudioSourceList {
    fn next(&mut self) -> Option<Box<dyn AudioSource>>;
}

/// A trait for resolvers which can create [AudioSource]s from string
/// descriptors. A plugin with the purpose of creating new ways of generating
/// audio to play with the bot usually registers at least one of these. As an
/// example, a plugin may register a resolver for WAV files. The resolver takes
/// as descriptors paths to WAV files and generates audio sources which decode
/// and stream those files.
pub trait AudioSourceResolver : Send + Sync {
    fn can_resolve(&self, descriptor: &str) -> bool;

    fn resolve(&self, descriptor: &str)
        -> Result<Box<dyn AudioSource + Send>, String>;
}

/// A trait for resolvers which can create effects from string descriptors.
/// Similarly to [AudioSourceResolver]s, these effects are realized as
/// [AudioSource]s, however they receive a child audio source whose output can
/// be modified, thus constituting an effect. As an example, a volume effect
/// could be realized by wrapping the child audio source and multiplying all
/// audio data it outputs by the volume number.
pub trait EffectResolver : Send + Sync {
    fn can_resolve(&self, descriptor: &str) -> bool;

    fn resolve(&self, descriptor: &str, child: Box<dyn AudioSource>)
        -> Result<Box<dyn AudioSource + Send>, String>;
}

pub trait AudioSourceListResolver : Send + Sync {
    fn can_resolve(&self, descriptor: &str) -> bool;

    fn resolve(&self, descriptor: &str)
        -> Result<Box<dyn AudioSourceList>, String>;
}

pub trait Plugin : std::any::Any + Send + Sync {
    fn load_plugin(&self) -> Result<(), String>;

    fn audio_source_resolvers(&self) -> Vec<Box<dyn AudioSourceResolver>>;

    fn effect_resolvers(&self) -> Vec<Box<dyn EffectResolver>>;

    fn audio_source_list_resolvers(&self)
        -> Vec<Box<dyn AudioSourceListResolver>>;
}

#[macro_export]
macro_rules! export_plugin {
    ($constructor:path) => {
        #[no_mangle]
        pub extern "Rust" fn _create_plugin() -> *mut dyn $crate::Plugin {
            let plugin = $constructor();
            let boxed: Box<dyn $crate::Plugin> = Box::new(plugin);

            Box::into_raw(boxed)
        }
    }
}
