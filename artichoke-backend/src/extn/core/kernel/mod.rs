use regex::Regex;
use std::str::FromStr;

use crate::convert::{Convert, TryConvert};
use crate::def::{ClassLike, Define};
use crate::eval::{Context, Eval};
use crate::extn::core::exception::{ArgumentError, LoadError, RubyException, RuntimeError};
use crate::sys;
use crate::value::{Value, ValueLike};
use crate::{Artichoke, ArtichokeError};

use mruby_sys::mrb_vtype::MRB_TT_FIXNUM;

pub mod require;

pub fn init(interp: &Artichoke) -> Result<(), ArtichokeError> {
    let warning = interp.0.borrow_mut().def_module::<Warning>("Warning", None);
    warning
        .borrow_mut()
        .add_method("warn", Warning::warn, sys::mrb_args_req(1));
    warning
        .borrow_mut()
        .add_self_method("warn", Warning::warn, sys::mrb_args_req(1));
    warning
        .borrow()
        .define(interp)
        .map_err(|_| ArtichokeError::New)?;
    let kernel = interp.0.borrow_mut().def_module::<Kernel>("Kernel", None);
    kernel
        .borrow_mut()
        .add_method("require", Kernel::require, sys::mrb_args_rest());
    kernel.borrow_mut().add_self_method(
        "require_relative",
        Kernel::require_relative,
        sys::mrb_args_rest(),
    );
    kernel
        .borrow_mut()
        .add_method("Integer", Kernel::integer, sys::mrb_args_req_and_opt(1, 2));
    kernel.borrow_mut().add_self_method(
        "Integer",
        Kernel::integer,
        sys::mrb_args_req_and_opt(1, 2),
    );
    kernel
        .borrow_mut()
        .add_self_method("load", Kernel::load, sys::mrb_args_rest());
    kernel
        .borrow_mut()
        .add_method("print", Kernel::print, sys::mrb_args_rest());
    kernel
        .borrow_mut()
        .add_method("puts", Kernel::puts, sys::mrb_args_rest());
    kernel
        .borrow_mut()
        .add_method("warn", Kernel::warn, sys::mrb_args_rest());
    kernel
        .borrow()
        .define(interp)
        .map_err(|_| ArtichokeError::New)?;
    interp.eval(include_str!("kernel.rb"))?;
    trace!("Patched Kernel#require onto interpreter");
    Ok(())
}

pub struct Warning;

impl Warning {
    unsafe extern "C" fn warn(mrb: *mut sys::mrb_state, _slf: sys::mrb_value) -> sys::mrb_value {
        let args = mrb_get_args!(mrb, *args);
        let interp = unwrap_interpreter!(mrb);
        let stderr = sys::mrb_gv_get(mrb, interp.0.borrow_mut().sym_intern("$stderr"));
        let stderr = Value::new(&interp, stderr);
        if !stderr.is_nil() {
            let args = args
                .iter()
                .map(|arg| Value::new(&interp, *arg))
                .collect::<Vec<_>>();
            // TODO: introduce a `unchecked_funcall` to propagate errors, GH-249.
            let _ = stderr.funcall::<Value>("print", args.as_ref(), None);
        }
        sys::mrb_sys_nil_value()
    }
}

pub struct Kernel;

impl Kernel {
    unsafe extern "C" fn require(mrb: *mut sys::mrb_state, _slf: sys::mrb_value) -> sys::mrb_value {
        let file = mrb_get_args!(mrb, required = 1);
        let interp = unwrap_interpreter!(mrb);
        let args = require::Args::validate_require(&interp, file);
        let result = args.and_then(|args| require::method::require(&interp, args));
        match result {
            Ok(req) => {
                let result = if let Some(req) = req.rust {
                    req(interp.clone())
                } else {
                    Ok(())
                };
                if result.is_ok() {
                    if let Some(contents) = req.ruby {
                        interp.unchecked_eval_with_context(contents, Context::new(req.file));
                    }
                    interp.convert(true).inner()
                } else {
                    LoadError::raisef(interp, "cannot load such file -- %S", vec![req.file])
                }
            }
            Err(require::ErrorReq::AlreadyRequired) => interp.convert(false).inner(),
            Err(require::ErrorReq::CannotLoad(file)) => {
                LoadError::raisef(interp, "cannot load such file -- %S", vec![file])
            }
            Err(require::ErrorReq::Fatal) => {
                RuntimeError::raise(interp, "fatal Kernel#require error")
            }
            Err(require::ErrorReq::NoImplicitConversionToString) => {
                ArgumentError::raise(interp, "No implicit conversion to String")
            }
        }
    }

    unsafe extern "C" fn integer(mrb: *mut sys::mrb_state, _slf: sys::mrb_value) -> sys::mrb_value {
        let interp = unwrap_interpreter!(mrb);
        let (arg, base, exception) = mrb_get_args!(mrb, required = 1, optional = 2);
        let mut buf = String::new();

        let mut radix: Option<i64> = None;
        if let Some(base) = base {
            radix = if let Ok(base) = interp.try_convert(Value::new(&interp, base)) {
                base
            } else {
                None
            };
        }

        if arg.tt == MRB_TT_FIXNUM && radix.is_some() {
            return ArgumentError::raise(interp, "base specified for non string value");
        }

        let arg = if let Ok(arg) = interp.try_convert(Value::new(&interp, arg)) {
            arg
        } else {
            ""
        };

        // If have consecutive embedded underscore, argument error!
        let multi_underscore_re = Regex::new(r"__+").unwrap();
        if arg.starts_with('_') || arg.ends_with('_') || multi_underscore_re.is_match(arg) {
            return ArgumentError::raisef(interp, "invalid value for Integer(): \"%S\"", vec![arg]);
        }

        // Remove embedded underscore `_`
        // because they represent error in `from_str_radix` & `from_str`
        let arg = arg.replace('_', "");

        // Remove leading and trailing white space,
        // because they represent error in `from_str_radix` & `from_str`.
        let arg = arg.trim();
        let arg = if arg.starts_with('-') {
            buf.push('-');
            &arg[1..]
        } else if arg.starts_with('+') {
            buf.push('+');
            &arg[1..]
        } else {
            arg
        };

        let mut raise_exception = true;
        if let Some(exception) = exception {
            raise_exception =
                if let Ok(raise_exception) = interp.try_convert(Value::new(&interp, exception)) {
                    raise_exception
                } else {
                    true
                };
        }

        let mut parsed_radix = None;

        // if `arg` is null byte, raise `ArgumentError`
        if arg.contains('\0') {
            return ArgumentError::raise(interp, "string contains null byte");
        }

        if arg.starts_with('0') && arg.len() > 2 {
            match &arg[0..2] {
                "0b" | "0B" => {
                    buf.push_str(&arg[2..]);
                    parsed_radix = Some(2);
                }
                "0o" | "0O" => {
                    buf.push_str(&arg[2..]);
                    parsed_radix = Some(8);
                }
                "0d" | "0D" => {
                    buf.push_str(&arg[2..]);
                    parsed_radix = Some(10);
                }
                "0x" | "0X" => {
                    buf.push_str(&arg[2..]);
                    parsed_radix = Some(16);
                }
                prefix if &prefix[0..1] == "0" => {
                    buf.push_str(&arg[1..]);
                    parsed_radix = Some(8);
                }
                _ => {}
            };
        } else {
            buf.push_str(arg);
        }

        let result = match (radix, parsed_radix) {
            (Some(radix), Some(parsed_radix)) => {
                if radix != parsed_radix {
                    return ArgumentError::raisef(
                        interp,
                        "invalid value for Integer(): \"%S\"",
                        vec![arg],
                    );
                }
                if let Ok(v) = i64::from_str_radix(buf.as_str(), radix as u32) {
                    v
                } else {
                    return ArgumentError::raisef(
                        interp,
                        "invalid value for Integer(): \"%S\"",
                        vec![arg],
                    );
                }
            }
            (Some(radix), None) => {
                if radix < 2 || radix > 36 {
                    return ArgumentError::raisef(interp, "invalid radix %S", vec![radix]);
                }
                if let Ok(v) = i64::from_str_radix(buf.as_str(), radix as u32) {
                    v
                } else {
                    return ArgumentError::raisef(
                        interp,
                        "invalid value for Integer(): \"%S\"",
                        vec![arg],
                    );
                }
            }
            (None, Some(radix)) => {
                if radix < 2 || radix > 36 {
                    return ArgumentError::raisef(interp, "invalid radix %S", vec![radix]);
                }
                if let Ok(v) = i64::from_str_radix(buf.as_str(), radix as u32) {
                    v
                } else {
                    return ArgumentError::raisef(
                        interp,
                        "invalid value for Integer(): \"%S\"",
                        vec![arg],
                    );
                }
            }
            (None, None) => {
                if let Ok(v) = i64::from_str(buf.as_str()) {
                    v
                } else {
                    return ArgumentError::raisef(
                        interp,
                        "invalid value for Integer(): \"%S\"",
                        vec![arg],
                    );
                }
            }
        };

        interp.convert(result).inner()
    }

    unsafe extern "C" fn load(mrb: *mut sys::mrb_state, _slf: sys::mrb_value) -> sys::mrb_value {
        let file = mrb_get_args!(mrb, required = 1);
        let interp = unwrap_interpreter!(mrb);
        let args = require::Args::validate_load(&interp, file);
        let result = args.and_then(|args| require::method::load(&interp, args));
        match result {
            Ok(req) => {
                let result = if let Some(req) = req.rust {
                    req(interp.clone())
                } else {
                    Ok(())
                };
                if result.is_ok() {
                    if let Some(contents) = req.ruby {
                        interp.unchecked_eval_with_context(contents, Context::new(req.file));
                    }
                    interp.convert(true).inner()
                } else {
                    LoadError::raisef(interp, "cannot load such file -- %S", vec![req.file])
                }
            }
            Err(require::ErrorLoad::CannotLoad(file)) => {
                LoadError::raisef(interp, "cannot load such file -- %S", vec![file])
            }
            Err(require::ErrorLoad::Fatal) => {
                RuntimeError::raise(interp, "fatal Kernel#load error")
            }
            Err(require::ErrorLoad::NoImplicitConversionToString) => {
                ArgumentError::raise(interp, "No implicit conversion to String")
            }
        }
    }

    unsafe extern "C" fn require_relative(
        mrb: *mut sys::mrb_state,
        _slf: sys::mrb_value,
    ) -> sys::mrb_value {
        let file = mrb_get_args!(mrb, required = 1);
        let interp = unwrap_interpreter!(mrb);
        let args = require::Args::validate_require(&interp, file);
        let result = args.and_then(|args| require::method::require_relative(&interp, args));
        match result {
            Ok(req) => {
                let result = if let Some(req) = req.rust {
                    req(interp.clone())
                } else {
                    Ok(())
                };
                if result.is_ok() {
                    if let Some(contents) = req.ruby {
                        interp.unchecked_eval_with_context(contents, Context::new(req.file));
                    }
                    interp.convert(true).inner()
                } else {
                    LoadError::raisef(interp, "cannot load such file -- %S", vec![req.file])
                }
            }
            Err(require::ErrorReq::AlreadyRequired) => interp.convert(false).inner(),
            Err(require::ErrorReq::CannotLoad(file)) => {
                LoadError::raisef(interp, "cannot load such file -- %S", vec![file])
            }
            Err(require::ErrorReq::Fatal) => {
                RuntimeError::raise(interp, "fatal Kernel#require error")
            }
            Err(require::ErrorReq::NoImplicitConversionToString) => {
                ArgumentError::raise(interp, "No implicit conversion to String")
            }
        }
    }

    unsafe extern "C" fn print(mrb: *mut sys::mrb_state, _slf: sys::mrb_value) -> sys::mrb_value {
        let args = mrb_get_args!(mrb, *args);
        let interp = unwrap_interpreter!(mrb);

        for value in args.iter() {
            let s = Value::new(&interp, *value).to_s();
            interp.0.borrow_mut().print(s.as_str());
        }
        sys::mrb_sys_nil_value()
    }

    unsafe extern "C" fn puts(mrb: *mut sys::mrb_state, _slf: sys::mrb_value) -> sys::mrb_value {
        fn do_puts(interp: &Artichoke, value: &Value) {
            if let Ok(array) = value.clone().try_into::<Vec<Value>>() {
                for value in array {
                    do_puts(interp, &value);
                }
            } else {
                let s = value.to_s();
                interp.0.borrow_mut().puts(s.as_str());
            }
        }

        let args = mrb_get_args!(mrb, *args);
        let interp = unwrap_interpreter!(mrb);
        if args.is_empty() {
            interp.0.borrow_mut().puts("");
        }
        for value in args.iter() {
            do_puts(&interp, &Value::new(&interp, *value));
        }
        sys::mrb_sys_nil_value()
    }

    unsafe extern "C" fn warn(mrb: *mut sys::mrb_state, _slf: sys::mrb_value) -> sys::mrb_value {
        let args = mrb_get_args!(mrb, *args);
        let interp = unwrap_interpreter!(mrb);

        for value in args.iter() {
            let mut string = Value::new(&interp, *value).to_s();
            if !string.ends_with('\n') {
                string = format!("{}\n", string);
            }
            Warning::warn(mrb, interp.convert(string).inner());
        }
        sys::mrb_sys_nil_value()
    }
}

#[cfg(test)]
mod tests {
    use crate::eval::Eval;
    use crate::file::File;
    use crate::load::LoadSources;
    use crate::value::ValueLike;
    use crate::{Artichoke, ArtichokeError};

    // Integration test for `Kernel::require`:
    //
    // - require side effects (e.g. ivar set or class def) effect the interpreter
    // - Successful first require returns `true`.
    // - Second require returns `false`.
    // - Second require does not cause require side effects.
    // - Require non-existing file raises and returns `nil`.
    #[test]
    fn require() {
        struct TestFile;

        impl File for TestFile {
            fn require(interp: Artichoke) -> Result<(), ArtichokeError> {
                interp.eval("@i = 255")?;
                Ok(())
            }
        }

        let interp = crate::interpreter().expect("init");
        interp
            .def_file_for_type::<_, TestFile>("file.rb")
            .expect("def file");
        let result = interp.eval("require 'file'").expect("eval");
        let require_result = result.try_into::<bool>();
        assert_eq!(require_result, Ok(true));
        let result = interp.eval("@i").expect("eval");
        let i_result = result.try_into::<i64>();
        assert_eq!(i_result, Ok(255));
        let result = interp.eval("@i = 1000; require 'file'").expect("eval");
        let second_require_result = result.try_into::<bool>();
        assert_eq!(second_require_result, Ok(false));
        let result = interp.eval("@i").expect("eval");
        let second_i_result = result.try_into::<i64>();
        assert_eq!(second_i_result, Ok(1000));
        let result = interp.eval("require 'non-existent-source'").map(|_| ());
        let expected = r#"
(eval):1: cannot load such file -- non-existent-source (LoadError)
(eval):1
            "#;
        assert_eq!(
            result,
            Err(ArtichokeError::Exec(expected.trim().to_owned()))
        );
    }

    #[test]
    fn require_absolute_path() {
        let interp = crate::interpreter().expect("init");
        interp
            .def_rb_source_file("/foo/bar/source.rb", "# a source file")
            .expect("def file");
        let result = interp.eval("require '/foo/bar/source.rb'").expect("value");
        assert!(result.try_into::<bool>().expect("convert"));
        let result = interp.eval("require '/foo/bar/source.rb'").expect("value");
        assert!(!result.try_into::<bool>().expect("convert"));
    }

    #[test]
    fn require_relative_with_dotted_path() {
        let interp = crate::interpreter().expect("init");
        interp
            .def_rb_source_file("/foo/bar/source.rb", "require_relative '../bar.rb'")
            .expect("def file");
        interp
            .def_rb_source_file("/foo/bar.rb", "# a source file")
            .expect("def file");
        let result = interp.eval("require '/foo/bar/source.rb'").expect("value");
        assert!(result.try_into::<bool>().expect("convert"));
    }

    #[test]
    fn require_directory() {
        let interp = crate::interpreter().expect("init");
        let result = interp.eval("require '/src'").map(|_| ());
        let expected = r#"
(eval):1: cannot load such file -- /src (LoadError)
(eval):1
        "#;
        assert_eq!(
            result,
            Err(ArtichokeError::Exec(expected.trim().to_owned()))
        );
    }

    #[test]
    fn require_path_defined_as_source_then_mrbfile() {
        struct Foo;
        impl File for Foo {
            fn require(interp: Artichoke) -> Result<(), ArtichokeError> {
                interp.eval("module Foo; RUST = 7; end")?;
                Ok(())
            }
        }
        let interp = crate::interpreter().expect("init");
        interp
            .def_rb_source_file("foo.rb", "module Foo; RUBY = 3; end")
            .expect("def");
        interp.def_file_for_type::<_, Foo>("foo.rb").expect("def");
        let result = interp.eval("require 'foo'").expect("eval");
        let result = result.try_into::<bool>().expect("convert");
        assert!(result, "successfully required foo.rb");
        let result = interp.eval("Foo::RUBY + Foo::RUST").expect("eval");
        let result = result.try_into::<i64>().expect("convert");
        assert_eq!(
            result, 10,
            "defined Ruby and Rust sources from single require"
        );
    }

    #[test]
    fn require_path_defined_as_mrbfile_then_source() {
        struct Foo;
        impl File for Foo {
            fn require(interp: Artichoke) -> Result<(), ArtichokeError> {
                interp.eval("module Foo; RUST = 7; end")?;
                Ok(())
            }
        }
        let interp = crate::interpreter().expect("init");
        interp.def_file_for_type::<_, Foo>("foo.rb").expect("def");
        interp
            .def_rb_source_file("foo.rb", "module Foo; RUBY = 3; end")
            .expect("def");
        let result = interp.eval("require 'foo'").expect("eval");
        let result = result.try_into::<bool>().expect("convert");
        assert!(result, "successfully required foo.rb");
        let result = interp.eval("Foo::RUBY + Foo::RUST").expect("eval");
        let result = result.try_into::<i64>().expect("convert");
        assert_eq!(
            result, 10,
            "defined Ruby and Rust sources from single require"
        );
    }

    #[test]
    #[allow(clippy::shadow_unrelated)]
    fn kernel_throw_catch() {
        // https://ruby-doc.org/core-2.6.3/Kernel.html#method-i-catch
        let interp = crate::interpreter().expect("init");
        let result = interp
            .eval("catch(1) { 123 }")
            .unwrap()
            .try_into::<i64>()
            .unwrap();
        assert_eq!(result, 123);
        let result = interp
            .eval("catch(1) { throw(1, 456) }")
            .unwrap()
            .try_into::<i64>()
            .unwrap();
        assert_eq!(result, 456);
        let result = interp
            .eval("catch(1) { throw(1) }")
            .unwrap()
            .try_into::<Option<i64>>()
            .unwrap();
        assert_eq!(result, None);
        let result = interp
            .eval("catch(1) {|x| x + 2 }")
            .unwrap()
            .try_into::<i64>()
            .unwrap();
        assert_eq!(result, 3);

        let result = interp
            .eval(
                r#"
catch do |obj_A|
  catch do |obj_B|
    throw(obj_B, 123)
    # puts "This puts is not reached"
  end

  # puts "This puts is displayed"
  456
end
            "#,
            )
            .unwrap()
            .try_into::<i64>()
            .unwrap();
        assert_eq!(result, 456);
        let result = interp
            .eval(
                r#"
catch do |obj_A|
  catch do |obj_B|
    throw(obj_A, 123)
    # puts "This puts is still not reached"
  end

  # puts "Now this puts is also not reached"
  456
end
            "#,
            )
            .unwrap()
            .try_into::<i64>()
            .unwrap();
        assert_eq!(result, 123);
    }
}
