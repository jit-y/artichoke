use std::borrow::Cow;
use std::error;
use std::ffi::c_void;
use std::fmt;

use crate::class_registry::ClassRegistry;
use crate::core::{ConvertMut, Eval, ReleaseMetadata};
use crate::exception::{Exception, RubyException};
use crate::extn;
use crate::extn::core::exception::Fatal;
use crate::ffi;
use crate::gc::{MrbGarbageCollection, State as GcState};
use crate::release_metadata::ReleaseMetadata as ArtichokeBackendReleaseMetadata;
use crate::state::State;
use crate::sys;
use crate::Artichoke;

/// Create and initialize an [`Artichoke`] interpreter.
///
/// This function creates a new [`State`], embeds it in the [`sys::mrb_state`],
/// initializes an [in memory virtual filesystem](crate::fs), and loads the
/// [`extn`] extensions to Ruby Core and Stdlib.
pub fn interpreter() -> Result<Artichoke, Exception> {
    interpreter_with_config(ArtichokeBackendReleaseMetadata::default())
}

/// Create and initialize an [`Artichoke`] interpreter with build metadata.
///
/// This function takes a customizable configuration for embedding metadata
/// about how Artichoke was built. Otherwise, it behaves identically to the
/// [`interpreter`] function.
#[allow(clippy::module_name_repetitions)]
pub fn interpreter_with_config<T>(config: T) -> Result<Artichoke, Exception>
where
    T: ReleaseMetadata,
{
    let state = State::new();
    let state = Box::new(state);
    let alloc_ud = Box::into_raw(state) as *mut c_void;
    let raw = unsafe { sys::mrb_open_allocf(Some(sys::mrb_default_allocf), alloc_ud) };
    debug!("Try initializing mrb interpreter");

    let mut interp = unsafe { ffi::from_user_data(raw).map_err(|_| InterpreterAllocError)? };

    if let Some(ref mut state) = interp.state {
        if let Some(mrb) = unsafe { raw.as_mut() } {
            state.try_init_parser(mrb)
        }
    }

    // mruby garbage collection relies on a fully initialized Array, which we
    // won't have until after `extn::core` is initialized. Disable GC before
    // init and clean up afterward.
    let prior_gc_state = interp.disable_gc();

    // Initialize Artichoke Core and Standard Library runtime
    debug!("Begin initializing Artichoke Core and Standard Library");
    extn::init(&mut interp, config)?;
    debug!("Succeeded initializing Artichoke Core and Standard Library");

    // Load mrbgems
    let mut arena = interp.create_arena_savepoint()?;

    unsafe {
        arena
            .interp()
            .with_ffi_boundary(|mrb| sys::mrb_init_mrbgems(mrb))?;
    }
    arena.restore();

    debug!(
        "Allocated mrb interpreter: {}",
        sys::mrb_sys_state_debug(unsafe { interp.mrb.as_mut() })
    );

    // mruby lazily initializes some core objects like top_self and generates a
    // lot of garbage on startup. Eagerly initialize the interpreter to provide
    // predictable initialization behavior.
    interp.create_arena_savepoint()?.interp().eval(&[])?;

    if let GcState::Enabled = prior_gc_state {
        interp.enable_gc();
        interp.full_gc();
    }

    Ok(interp)
}

#[derive(Default, Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[allow(clippy::module_name_repetitions)]
pub struct InterpreterAllocError;

impl InterpreterAllocError {
    /// Constructs a new, default `InterpreterAllocError`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl fmt::Display for InterpreterAllocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Failed to allocate Artichoke interpreter")
    }
}

impl error::Error for InterpreterAllocError {}

impl RubyException for InterpreterAllocError {
    fn message(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(b"Failed to allocate Artichoke Ruby interpreter")
    }

    fn name(&self) -> Cow<'_, str> {
        "fatal".into()
    }

    fn vm_backtrace(&self, interp: &mut Artichoke) -> Option<Vec<Vec<u8>>> {
        let _ = interp;
        None
    }

    fn as_mrb_value(&self, interp: &mut Artichoke) -> Option<sys::mrb_value> {
        let message = interp.convert_mut(self.message());
        let value = interp.new_instance::<Fatal>(&[message]).ok().flatten()?;
        Some(value.inner())
    }
}

impl From<InterpreterAllocError> for Exception {
    fn from(exception: InterpreterAllocError) -> Self {
        Self::from(Box::<dyn RubyException>::from(exception))
    }
}

impl From<Box<InterpreterAllocError>> for Exception {
    fn from(exception: Box<InterpreterAllocError>) -> Self {
        Self::from(Box::<dyn RubyException>::from(exception))
    }
}

impl From<InterpreterAllocError> for Box<dyn RubyException> {
    fn from(exception: InterpreterAllocError) -> Box<dyn RubyException> {
        Box::new(exception)
    }
}

impl From<Box<InterpreterAllocError>> for Box<dyn RubyException> {
    fn from(exception: Box<InterpreterAllocError>) -> Box<dyn RubyException> {
        exception
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn open_close() {
        let interp = super::interpreter().unwrap();
        interp.close();
    }
}
