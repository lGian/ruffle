use crate::avm1::activation::Activation;
use crate::avm1::error::Error;
use crate::avm1::function::{Executable, FunctionObject};
use crate::avm1::listeners::SystemListeners;
use crate::avm1::{Object, ScriptObject, TObject, UpdateContext, Value};
use enumset::EnumSet;
use gc_arena::Collect;
use gc_arena::MutationContext;
use rand::Rng;
use std::f64;

mod array;
pub(crate) mod boolean;
pub(crate) mod button;
mod color;
mod color_transform;
pub(crate) mod context_menu;
pub(crate) mod context_menu_item;
pub(crate) mod display_object;
pub(crate) mod error;
mod function;
mod key;
mod math;
mod matrix;
pub(crate) mod mouse;
pub(crate) mod movie_clip;
mod movie_clip_loader;
pub(crate) mod number;
mod object;
mod point;
mod rectangle;
pub(crate) mod shared_object;
mod sound;
mod stage;
pub(crate) mod string;
pub(crate) mod system;
pub(crate) mod system_capabilities;
pub(crate) mod system_ime;
pub(crate) mod system_security;
pub(crate) mod text_field;
mod text_format;
mod xml;

pub fn random<'gc>(
    _activation: &mut Activation<'_, 'gc>,
    action_context: &mut UpdateContext<'_, 'gc, '_>,
    _this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    match args.get(0) {
        Some(Value::Number(max)) => Ok(action_context.rng.gen_range(0.0f64, max).floor().into()),
        _ => Ok(Value::Undefined), //TODO: Shouldn't this be an error condition?
    }
}

pub fn is_nan<'gc>(
    activation: &mut Activation<'_, 'gc>,
    action_context: &mut UpdateContext<'_, 'gc, '_>,
    _this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(val) = args.get(0) {
        Ok(val
            .coerce_to_f64(activation, action_context)?
            .is_nan()
            .into())
    } else {
        Ok(true.into())
    }
}

pub fn get_infinity<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _action_context: &mut UpdateContext<'_, 'gc, '_>,
    _this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if activation.current_swf_version() > 4 {
        Ok(f64::INFINITY.into())
    } else {
        Ok(Value::Undefined)
    }
}

pub fn get_nan<'gc>(
    activation: &mut Activation<'_, 'gc>,
    _action_context: &mut UpdateContext<'_, 'gc, '_>,
    _this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if activation.current_swf_version() > 4 {
        Ok(f64::NAN.into())
    } else {
        Ok(Value::Undefined)
    }
}

pub fn set_interval<'a, 'gc>(
    activation: &mut Activation<'_, 'gc>,
    context: &mut UpdateContext<'a, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    create_timer(activation, context, this, args, false)
}

pub fn set_timeout<'a, 'gc>(
    activation: &mut Activation<'_, 'gc>,
    context: &mut UpdateContext<'a, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    create_timer(activation, context, this, args, true)
}

pub fn create_timer<'a, 'gc>(
    activation: &mut Activation<'_, 'gc>,
    context: &mut UpdateContext<'a, 'gc, '_>,
    _this: Object<'gc>,
    args: &[Value<'gc>],
    is_timeout: bool,
) -> Result<Value<'gc>, Error<'gc>> {
    // `setInterval` was added in Flash Player 6 but is not version-gated.
    use crate::avm1::timer::TimerCallback;
    let (callback, i) = match args.get(0) {
        Some(Value::Object(o)) if o.as_executable().is_some() => (TimerCallback::Function(*o), 1),
        Some(Value::Object(o)) => (
            TimerCallback::Method {
                this: *o,
                method_name: args
                    .get(1)
                    .unwrap_or(&Value::Undefined)
                    .coerce_to_string(activation, context)?
                    .to_string(),
            },
            2,
        ),
        _ => return Ok(Value::Undefined),
    };

    let interval = match args.get(i) {
        Some(Value::Undefined) | None => return Ok(Value::Undefined),
        Some(value) => value.coerce_to_i32(activation, context)?,
    };
    let params = if let Some(params) = args.get(i + 1..) {
        params.to_vec()
    } else {
        vec![]
    };

    let id = context
        .timers
        .add_timer(callback, interval, params, is_timeout);

    Ok(id.into())
}

pub fn clear_interval<'a, 'gc>(
    activation: &mut Activation<'_, 'gc>,
    context: &mut UpdateContext<'a, 'gc, '_>,
    _this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let id = args
        .get(0)
        .unwrap_or(&Value::Undefined)
        .coerce_to_i32(activation, context)?;
    if !context.timers.remove(id) {
        log::info!("clearInterval: Timer {} does not exist", id);
    }

    Ok(Value::Undefined)
}

pub fn update_after_event<'a, 'gc>(
    _activation: &mut Activation<'_, 'gc>,
    context: &mut UpdateContext<'a, 'gc, '_>,
    _this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    *context.needs_render = true;

    Ok(Value::Undefined)
}

/// This structure represents all system builtins that are used regardless of
/// whatever the hell happens to `_global`. These are, of course,
/// user-modifiable.
#[derive(Collect, Clone)]
#[collect(no_drop)]
pub struct SystemPrototypes<'gc> {
    pub button: Object<'gc>,
    pub object: Object<'gc>,
    pub function: Object<'gc>,
    pub movie_clip: Object<'gc>,
    pub sound: Object<'gc>,
    pub text_field: Object<'gc>,
    pub text_format: Object<'gc>,
    pub array: Object<'gc>,
    pub xml_node: Object<'gc>,
    pub string: Object<'gc>,
    pub number: Object<'gc>,
    pub boolean: Object<'gc>,
    pub matrix: Object<'gc>,
    pub point: Object<'gc>,
    pub rectangle: Object<'gc>,
    pub rectangle_constructor: Object<'gc>,
    pub shared_object: Object<'gc>,
    pub color_transform: Object<'gc>,
    pub context_menu: Object<'gc>,
    pub context_menu_item: Object<'gc>,
}

/// Initialize default global scope and builtins for an AVM1 instance.
pub fn create_globals<'gc>(
    gc_context: MutationContext<'gc, '_>,
) -> (SystemPrototypes<'gc>, Object<'gc>, SystemListeners<'gc>) {
    let object_proto = ScriptObject::object_cell(gc_context, None);
    let function_proto = function::create_proto(gc_context, object_proto);

    object::fill_proto(gc_context, object_proto, function_proto);

    let button_proto: Object<'gc> = button::create_proto(gc_context, object_proto, function_proto);

    let movie_clip_proto: Object<'gc> =
        movie_clip::create_proto(gc_context, object_proto, function_proto);

    let movie_clip_loader_proto: Object<'gc> =
        movie_clip_loader::create_proto(gc_context, object_proto, function_proto);

    let sound_proto: Object<'gc> = sound::create_proto(gc_context, object_proto, function_proto);

    let text_field_proto: Object<'gc> =
        text_field::create_proto(gc_context, object_proto, function_proto);
    let text_format_proto: Object<'gc> =
        text_format::create_proto(gc_context, object_proto, function_proto);

    let array_proto: Object<'gc> = array::create_proto(gc_context, object_proto, function_proto);

    let color_proto: Object<'gc> = color::create_proto(gc_context, object_proto, function_proto);

    let error_proto: Object<'gc> = error::create_proto(gc_context, object_proto, function_proto);

    let xmlnode_proto: Object<'gc> =
        xml::create_xmlnode_proto(gc_context, object_proto, function_proto);

    let xml_proto: Object<'gc> = xml::create_xml_proto(gc_context, xmlnode_proto, function_proto);

    let string_proto: Object<'gc> = string::create_proto(gc_context, object_proto, function_proto);
    let number_proto: Object<'gc> = number::create_proto(gc_context, object_proto, function_proto);
    let boolean_proto: Object<'gc> =
        boolean::create_proto(gc_context, object_proto, function_proto);
    let matrix_proto: Object<'gc> = matrix::create_proto(gc_context, object_proto, function_proto);
    let point_proto: Object<'gc> = point::create_proto(gc_context, object_proto, function_proto);
    let rectangle_proto: Object<'gc> =
        rectangle::create_proto(gc_context, object_proto, function_proto);
    let color_transform_proto: Object<'gc> =
        color_transform::create_proto(gc_context, object_proto, function_proto);

    //TODO: These need to be constructors and should also set `.prototype` on each one
    let object = object::create_object_object(gc_context, object_proto, function_proto);

    let context_menu_proto = context_menu::create_proto(gc_context, object_proto, function_proto);
    let context_menu_item_proto =
        context_menu_item::create_proto(gc_context, object_proto, function_proto);

    let button = FunctionObject::function(
        gc_context,
        Executable::Native(button::constructor),
        Some(function_proto),
        Some(button_proto),
    );
    let color = FunctionObject::function(
        gc_context,
        Executable::Native(color::constructor),
        Some(function_proto),
        Some(color_proto),
    );
    let error = FunctionObject::function(
        gc_context,
        Executable::Native(error::constructor),
        Some(function_proto),
        Some(error_proto),
    );
    let function = FunctionObject::function(
        gc_context,
        Executable::Native(function::constructor),
        Some(function_proto),
        Some(function_proto),
    );
    let movie_clip = FunctionObject::function(
        gc_context,
        Executable::Native(movie_clip::constructor),
        Some(function_proto),
        Some(movie_clip_proto),
    );
    let movie_clip_loader = FunctionObject::function(
        gc_context,
        Executable::Native(movie_clip_loader::constructor),
        Some(function_proto),
        Some(movie_clip_loader_proto),
    );
    let sound = FunctionObject::function(
        gc_context,
        Executable::Native(sound::constructor),
        Some(function_proto),
        Some(sound_proto),
    );
    let text_field = FunctionObject::function(
        gc_context,
        Executable::Native(text_field::constructor),
        Some(function_proto),
        Some(text_field_proto),
    );
    let text_format = FunctionObject::function(
        gc_context,
        Executable::Native(text_format::constructor),
        Some(function_proto),
        Some(text_format_proto),
    );
    let array = array::create_array_object(gc_context, Some(array_proto), Some(function_proto));
    let xmlnode = FunctionObject::function(
        gc_context,
        Executable::Native(xml::xmlnode_constructor),
        Some(function_proto),
        Some(xmlnode_proto),
    );
    let xml = FunctionObject::function(
        gc_context,
        Executable::Native(xml::xml_constructor),
        Some(function_proto),
        Some(xml_proto),
    );
    let string = string::create_string_object(gc_context, Some(string_proto), Some(function_proto));
    let number = number::create_number_object(gc_context, Some(number_proto), Some(function_proto));
    let boolean =
        boolean::create_boolean_object(gc_context, Some(boolean_proto), Some(function_proto));

    let flash = ScriptObject::object(gc_context, Some(object_proto));
    let geom = ScriptObject::object(gc_context, Some(object_proto));
    let matrix = matrix::create_matrix_object(gc_context, Some(matrix_proto), Some(function_proto));

    let point = point::create_point_object(gc_context, Some(point_proto), Some(function_proto));
    let rectangle =
        rectangle::create_rectangle_object(gc_context, Some(rectangle_proto), Some(function_proto));

    flash.define_value(gc_context, "geom", geom.into(), EnumSet::empty());
    geom.define_value(gc_context, "Matrix", matrix.into(), EnumSet::empty());
    geom.define_value(gc_context, "Point", point.into(), EnumSet::empty());
    geom.define_value(gc_context, "Rectangle", rectangle.into(), EnumSet::empty());
    geom.define_value(
        gc_context,
        "ColorTransform",
        FunctionObject::function(
            gc_context,
            Executable::Native(color_transform::constructor),
            Some(function_proto),
            Some(color_transform_proto),
        )
        .into(),
        EnumSet::empty(),
    );

    let listeners = SystemListeners::new(gc_context, Some(array_proto));

    let mut globals = ScriptObject::bare_object(gc_context);
    globals.define_value(gc_context, "flash", flash.into(), EnumSet::empty());
    globals.define_value(gc_context, "Array", array.into(), EnumSet::empty());
    globals.define_value(gc_context, "Button", button.into(), EnumSet::empty());
    globals.define_value(gc_context, "Color", color.into(), EnumSet::empty());
    globals.define_value(gc_context, "Error", error.into(), EnumSet::empty());
    globals.define_value(gc_context, "Object", object.into(), EnumSet::empty());
    globals.define_value(gc_context, "Function", function.into(), EnumSet::empty());
    globals.define_value(gc_context, "MovieClip", movie_clip.into(), EnumSet::empty());
    globals.define_value(
        gc_context,
        "MovieClipLoader",
        movie_clip_loader.into(),
        EnumSet::empty(),
    );
    globals.define_value(gc_context, "Sound", sound.into(), EnumSet::empty());
    globals.define_value(gc_context, "TextField", text_field.into(), EnumSet::empty());
    globals.define_value(
        gc_context,
        "TextFormat",
        text_format.into(),
        EnumSet::empty(),
    );
    globals.define_value(gc_context, "XMLNode", xmlnode.into(), EnumSet::empty());
    globals.define_value(gc_context, "XML", xml.into(), EnumSet::empty());
    globals.define_value(gc_context, "String", string.into(), EnumSet::empty());
    globals.define_value(gc_context, "Number", number.into(), EnumSet::empty());
    globals.define_value(gc_context, "Boolean", boolean.into(), EnumSet::empty());

    let shared_object_proto = shared_object::create_proto(gc_context, object_proto, function_proto);

    let shared_obj = shared_object::create_shared_object_object(
        gc_context,
        Some(shared_object_proto),
        Some(function_proto),
    );
    globals.define_value(
        gc_context,
        "SharedObject",
        shared_obj.into(),
        EnumSet::empty(),
    );

    globals.define_value(
        gc_context,
        "ContextMenu",
        FunctionObject::function(
            gc_context,
            Executable::Native(context_menu::constructor),
            Some(function_proto),
            Some(context_menu_proto),
        )
        .into(),
        EnumSet::empty(),
    );

    globals.define_value(
        gc_context,
        "ContextMenuItem",
        FunctionObject::function(
            gc_context,
            Executable::Native(context_menu_item::constructor),
            Some(function_proto),
            Some(context_menu_item_proto),
        )
        .into(),
        EnumSet::empty(),
    );

    let system_security =
        system_security::create(gc_context, Some(object_proto), Some(function_proto));
    let system_capabilities = system_capabilities::create(gc_context, Some(object_proto));
    let system_ime = system_ime::create(
        gc_context,
        Some(object_proto),
        Some(function_proto),
        &listeners.ime,
    );

    let system = system::create(
        gc_context,
        Some(object_proto),
        Some(function_proto),
        system_security,
        system_capabilities,
        system_ime,
    );
    globals.define_value(gc_context, "System", system.into(), EnumSet::empty());

    globals.define_value(
        gc_context,
        "Math",
        Value::Object(math::create(
            gc_context,
            Some(object_proto),
            Some(function_proto),
        )),
        EnumSet::empty(),
    );
    globals.define_value(
        gc_context,
        "Mouse",
        Value::Object(mouse::create_mouse_object(
            gc_context,
            Some(object_proto),
            Some(function_proto),
            &listeners.mouse,
        )),
        EnumSet::empty(),
    );
    globals.define_value(
        gc_context,
        "Key",
        Value::Object(key::create_key_object(
            gc_context,
            Some(object_proto),
            Some(function_proto),
        )),
        EnumSet::empty(),
    );
    globals.define_value(
        gc_context,
        "Stage",
        Value::Object(stage::create_stage_object(
            gc_context,
            Some(object_proto),
            Some(array_proto),
            Some(function_proto),
        )),
        EnumSet::empty(),
    );
    globals.force_set_function(
        "isNaN",
        is_nan,
        gc_context,
        EnumSet::empty(),
        Some(function_proto),
    );
    globals.force_set_function(
        "random",
        random,
        gc_context,
        EnumSet::empty(),
        Some(function_proto),
    );
    globals.force_set_function(
        "ASSetPropFlags",
        object::as_set_prop_flags,
        gc_context,
        EnumSet::empty(),
        Some(function_proto),
    );
    globals.force_set_function(
        "clearInterval",
        clear_interval,
        gc_context,
        EnumSet::empty(),
        Some(function_proto),
    );
    globals.force_set_function(
        "setInterval",
        set_interval,
        gc_context,
        EnumSet::empty(),
        Some(function_proto),
    );
    globals.force_set_function(
        "setTimeout",
        set_timeout,
        gc_context,
        EnumSet::empty(),
        Some(function_proto),
    );
    globals.force_set_function(
        "updateAfterEvent",
        update_after_event,
        gc_context,
        EnumSet::empty(),
        Some(function_proto),
    );

    globals.add_property(
        gc_context,
        "NaN",
        Executable::Native(get_nan),
        None,
        EnumSet::empty(),
    );
    globals.add_property(
        gc_context,
        "Infinity",
        Executable::Native(get_infinity),
        None,
        EnumSet::empty(),
    );

    (
        SystemPrototypes {
            button: button_proto,
            object: object_proto,
            function: function_proto,
            movie_clip: movie_clip_proto,
            sound: sound_proto,
            text_field: text_field_proto,
            text_format: text_format_proto,
            array: array_proto,
            xml_node: xmlnode_proto,
            string: string_proto,
            number: number_proto,
            boolean: boolean_proto,
            matrix: matrix_proto,
            point: point_proto,
            rectangle: rectangle_proto,
            rectangle_constructor: rectangle,
            shared_object: shared_object_proto,
            color_transform: color_transform_proto,
            context_menu: context_menu_proto,
            context_menu_item: context_menu_item_proto,
        },
        globals.into(),
        listeners,
    )
}

#[cfg(test)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::*;

    fn setup<'gc>(
        _activation: &mut Activation<'_, 'gc>,
        context: &mut UpdateContext<'_, 'gc, '_>,
    ) -> Object<'gc> {
        create_globals(context.gc_context).1
    }

    test_method!(boolean_function, "Boolean", setup,
        [19] => {
            [true] => true,
            [false] => false,
            [10.0] => true,
            [-10.0] => true,
            [0.0] => false,
            [std::f64::INFINITY] => true,
            [std::f64::NAN] => false,
            [""] => false,
            ["Hello"] => true,
            [" "] => true,
            ["0"] => true,
            ["1"] => true,
            [Value::Undefined] => false,
            [Value::Null] => false,
            [] => Value::Undefined
        },
        [6] => {
            [true] => true,
            [false] => false,
            [10.0] => true,
            [-10.0] => true,
            [0.0] => false,
            [std::f64::INFINITY] => true,
            [std::f64::NAN] => false,
            [""] => false,
            ["Hello"] => false,
            [" "] => false,
            ["0"] => false,
            ["1"] => true,
            [Value::Undefined] => false,
            [Value::Null] => false,
            [] => Value::Undefined
        }
    );

    test_method!(is_nan_function, "isNaN", setup,
        [19] => {
            [true] => false,
            [false] => false,
            [10.0] => false,
            [-10.0] => false,
            [0.0] => false,
            [std::f64::INFINITY] => false,
            [std::f64::NAN] => true,
            [""] => true,
            ["Hello"] => true,
            [" "] => true,
            ["  5  "] => true,
            ["0"] => false,
            ["1"] => false,
            ["Infinity"] => true,
            ["100a"] => true,
            ["0x10"] => false,
            ["0xhello"] => true,
            ["0x1999999981ffffff"] => false,
            ["0xUIXUIDFKHJDF012345678"] => true,
            ["123e-1"] => false,
            [] => true
        }
    );

    test_method!(number_function, "Number", setup,
        [5, 6] => {
            [true] => 1.0,
            [false] => 0.0,
            [10.0] => 10.0,
            [-10.0] => -10.0,
            ["true"] => std::f64::NAN,
            ["false"] => std::f64::NAN,
            [1.0] => 1.0,
            [0.0] => 0.0,
            [0.000] => 0.0,
            ["0.000"] => 0.0,
            ["True"] => std::f64::NAN,
            ["False"] => std::f64::NAN,
            [std::f64::NAN] => std::f64::NAN,
            [std::f64::INFINITY] => std::f64::INFINITY,
            [std::f64::NEG_INFINITY] => std::f64::NEG_INFINITY,
            [" 12"] => 12.0,
            [" \t\r\n12"] => 12.0,
            ["\u{A0}12"] => std::f64::NAN,
            [" 0x12"] => std::f64::NAN,
            ["01.2"] => 1.2,
            [""] => std::f64::NAN,
            ["Hello"] => std::f64::NAN,
            [" "] => std::f64::NAN,
            ["  5  "] => std::f64::NAN,
            ["0"] => 0.0,
            ["1"] => 1.0,
            ["Infinity"] => std::f64::NAN,
            ["100a"] => std::f64::NAN,
            ["0xhello"] => std::f64::NAN,
            ["123e-1"] => 12.3,
            ["0xUIXUIDFKHJDF012345678"] => std::f64::NAN,
            [] => 0.0
        },
        [5] => {
            ["0x12"] => std::f64::NAN,
            ["0x10"] => std::f64::NAN,
            ["0x1999999981ffffff"] => std::f64::NAN,
            ["010"] => 10,
            ["-010"] => -10,
            ["+010"] => 10,
            [" 010"] => 10,
            [" -010"] => -10,
            [" +010"] => 10,
            ["037777777777"] => 37777777777.0,
            ["-037777777777"] => -37777777777.0
        },
        [6, 7] => {
            ["0x12"] => 18.0,
            ["0x10"] => 16.0,
            ["-0x10"] => std::f64::NAN,
            ["0x1999999981ffffff"] => -2113929217.0,
            ["010"] => 8,
            ["-010"] => -8,
            ["+010"] => 8,
            [" 010"] => 10,
            [" -010"] => -10,
            [" +010"] => 10,
            ["037777777777"] => -1,
            ["-037777777777"] => 1
        },
        [5, 6] => {
            [Value::Undefined] => 0.0,
            [Value::Null] => 0.0
        },
        [7] => {
            [Value::Undefined] => std::f64::NAN,
            [Value::Null] => std::f64::NAN
        }
    );
}
