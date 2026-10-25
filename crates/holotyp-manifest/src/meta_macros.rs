/// The normalized key of a Rust type, used as a table key; registers the type
/// for `meta::initial_cast` on the way.
macro_rules! k {
    ($ty:ty) => {
        crate::meta::register_key::<$ty>()
    };
}

/// `CastInfo::Type` for a Rust type (its key), registering the type for
/// `meta::initial_cast`.
macro_rules! t {
    ($ty:ty) => {
        crate::meta::register_type::<$ty>()
    };
}

/// An optional dict field.
macro_rules! f {
    ($name:literal, $input:expr) => {
        crate::cast_type::Field {
            name: String::from($name),
            input: $input,
            required: false,
        }
    };
}

/// A required dict field.
macro_rules! req {
    ($name:literal, $input:expr) => {
        crate::cast_type::Field {
            name: String::from($name),
            input: $input,
            required: true,
        }
    };
}

pub(crate) use {f, k, req, t};
