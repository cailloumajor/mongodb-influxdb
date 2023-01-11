use std::fmt;

use lazy_static::lazy_static;
use mongodb::bson::Bson;
use tracing::{error, info_span};

use super::Replacer;

lazy_static! {
    static ref FIELD_VALUE_REPLACER: Replacer =
        Replacer::new(&[r#"""#, r#"\"#], &[r#"\""#, r#"\\"#]);
}

#[derive(Debug, PartialEq)]
pub(super) enum FieldValue {
    Float(f64),
    Integer(i64),
    UInteger(u64),
    String(String),
    Boolean(bool),
}

impl fmt::Display for FieldValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Float(float) => {
                let mut buffer = ryu::Buffer::new();
                let out = buffer.format(float);
                f.write_str(out)
            }
            Self::Integer(i) => {
                let mut buffer = itoa::Buffer::new();
                let out = buffer.format(i);
                write!(f, "{}i", out)
            }
            Self::UInteger(i) => {
                let mut buffer = itoa::Buffer::new();
                let out = buffer.format(i);
                write!(f, "{}u", out)
            }
            Self::String(ref s) => {
                let escaped = FIELD_VALUE_REPLACER.replace_all(s);
                f.write_str(r#"""#)?;
                f.write_str(&escaped)?;
                f.write_str(r#"""#)
            }
            Self::Boolean(b) => fmt::Display::fmt(&b, f),
        }
    }
}

impl TryFrom<Bson> for FieldValue {
    type Error = &'static str;

    fn try_from(value: Bson) -> Result<Self, Self::Error> {
        let _entered = info_span!("field_value_from_bson").entered();

        match value {
            Bson::Double(d) => Ok(Self::Float(d)),
            Bson::String(s) => Ok(Self::String(s)),
            Bson::Boolean(b) => Ok(Self::Boolean(b)),
            Bson::Int32(i) => Ok(Self::Integer(i.into())),
            Bson::Int64(i) => Ok(Self::Integer(i)),
            unsupported => {
                error!(kind="unsuppported", bson_type=?unsupported);
                Err("unsupported BSON type")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_double() {
        let bson = Bson::Double(42.5);

        let result = FieldValue::try_from(bson).expect("conversion error");

        assert_eq!(result, FieldValue::Float(42.5));
    }

    #[test]
    fn from_string() {
        let bson = Bson::String("astring".into());

        let result = FieldValue::try_from(bson).expect("conversion error");

        assert_eq!(result, FieldValue::String("astring".into()));
    }

    #[test]
    fn from_boolean() {
        let bson = Bson::Boolean(true);

        let result = FieldValue::try_from(bson).expect("conversion error");

        assert_eq!(result, FieldValue::Boolean(true));
    }

    #[test]
    fn from_int32() {
        let bson = Bson::Int32(42);

        let result = FieldValue::try_from(bson).expect("conversion error");

        assert_eq!(result, FieldValue::Integer(42));
    }

    #[test]
    fn from_int64() {
        let bson = Bson::Int64(42000);

        let result = FieldValue::try_from(bson).expect("conversion error");

        assert_eq!(result, FieldValue::Integer(42000));
    }

    #[test]
    fn display_float() {
        let field_value = FieldValue::Float(37.42);

        assert_eq!(field_value.to_string(), "37.42");
    }

    #[test]
    fn display_float_scientific() {
        let field_value = FieldValue::Float(0.000000000000000000000000001);

        assert_eq!(field_value.to_string(), "1e-27");
    }

    #[test]
    fn display_integer() {
        let field_value = FieldValue::Integer(-546130);

        assert_eq!(field_value.to_string(), "-546130i");
    }

    #[test]
    fn display_uinteger() {
        let field_value = FieldValue::UInteger(549844);

        assert_eq!(field_value.to_string(), "549844u");
    }

    #[test]
    fn display_string() {
        let to_escape = r#"some "text" wi\th \\ escapes"#;
        let field_value = FieldValue::String(to_escape.into());

        let expected = r#""some \"text\" wi\\th \\\\ escapes""#;
        assert_eq!(field_value.to_string(), expected);
    }

    #[test]
    fn display_true() {
        let field_value = FieldValue::Boolean(true);

        assert_eq!(field_value.to_string(), "true");
    }

    #[test]
    fn display_false() {
        let field_value = FieldValue::Boolean(false);

        assert_eq!(field_value.to_string(), "false");
    }
}
