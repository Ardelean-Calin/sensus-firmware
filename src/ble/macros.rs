#[macro_export]
macro_rules! bthome_length {
    ($field:expr) => {
        if let Some(f) = $field.as_ref() {
            f.length()
        } else {
            0
        }
    };
}

#[macro_export]
macro_rules! extend_if_some {
    ($dest:expr, $field:expr) => {
        if let Some(mut field) = $field {
            let arr = defmt::unwrap!(field.as_vec());
            defmt::unwrap!($dest.extend_from_slice(arr.as_slice()));
        }
    };
}
