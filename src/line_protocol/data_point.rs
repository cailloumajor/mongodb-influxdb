use std::collections::BTreeMap;
use std::fmt;
use std::sync::OnceLock;

use tracing::{error, info_span};

use crate::mongodb::DataDocument;

use super::Replacer;
use super::field_value::FieldValue;

static MEASUREMENT_REPLACER: OnceLock<Replacer> = OnceLock::new();
static TAG_KV_FIELD_K_REPLACER: OnceLock<Replacer> = OnceLock::new();

pub(crate) struct DataPoint {
    measurement: String,
    tags: BTreeMap<String, String>,
    fields: BTreeMap<String, FieldValue>,
    /// Unix timestamp in second precision.
    timestamp: u64,
}

impl DataPoint {
    pub(crate) fn create(
        data_doc: DataDocument,
        measurement: String,
        timestamp: u64, // in seconds
    ) -> Self {
        let _entered = info_span!("data_point_create").entered();

        let mut fields: BTreeMap<String, FieldValue> = BTreeMap::new();
        for (key, value) in data_doc.val {
            match value.try_into() {
                Ok(field_value) => {
                    fields.insert(key, field_value);
                }
                Err(err) => {
                    error!(
                        during = "DataPoint creation",
                        doc_id = data_doc.id,
                        field = key,
                        err
                    );
                }
            }
        }

        let tags = [("id".into(), data_doc.id)].into();

        Self {
            measurement,
            tags,
            fields,
            timestamp,
        }
    }
}

impl fmt::Display for DataPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let measurement_replacer =
            MEASUREMENT_REPLACER.get_or_init(|| Replacer::new(&[",", " "], &[r"\,", r"\ "]));
        let tag_field_replacer = TAG_KV_FIELD_K_REPLACER
            .get_or_init(|| Replacer::new(&[",", "=", " "], &[r"\,", r"\=", r"\ "]));
        let escaped_measurement = measurement_replacer.replace_all(&self.measurement);
        f.write_str(&escaped_measurement)?;

        for (key, value) in &self.tags {
            f.write_str(",")?;
            let escaped_key = tag_field_replacer.replace_all(key);
            f.write_str(&escaped_key)?;
            f.write_str("=")?;
            let escaped_value = tag_field_replacer.replace_all(value);
            f.write_str(&escaped_value)?;
        }

        f.write_str(" ")?;

        let mut fields = self.fields.iter().peekable();
        while let Some((key, value)) = fields.next() {
            let escaped_key = tag_field_replacer.replace_all(key);
            f.write_str(&escaped_key)?;
            f.write_str("=")?;
            fmt::Display::fmt(value, f)?;
            if fields.peek().is_some() {
                f.write_str(",")?;
            }
        }

        f.write_str(" ")?;

        let mut buffer = itoa::Buffer::new();
        let out = buffer.format(self.timestamp);
        f.write_str(out)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::{self, DateTime, doc};

    use crate::mongodb::DataDocument;

    use super::*;

    #[test]
    fn create() {
        let now_secs = DateTime::now().timestamp_millis() / 1000;
        let document = doc! {
            "_id": "anid",
            "updatedSince": 0,
            "val": {
                "first": true,
                "second": "a_string",
                "third": 37.5,
                "fourth": 42,
            },
        };
        let data_document: DataDocument = bson::deserialize_from_document(document).unwrap();
        let measurement = String::from("some_measurement");

        let data_point = DataPoint::create(data_document, measurement, now_secs as u64);

        assert_eq!(data_point.measurement, "some_measurement");
        assert_eq!(data_point.tags["id"], "anid");
        assert_eq!(data_point.fields["first"], FieldValue::Boolean(true));
        assert_eq!(
            data_point.fields["second"],
            FieldValue::String("a_string".into())
        );
        assert_eq!(data_point.fields["third"], FieldValue::Float(37.5));
        assert_eq!(data_point.fields["fourth"], FieldValue::Integer(42));
    }

    #[test]
    fn display_datapoint() {
        let tags = [
            ("some, tagkey".into(), "a=tagvalue".into()),
            ("a=tagkey".into(), "some, tagvalue".into()),
            ("othertagkey".into(), "otherval".into()),
        ]
        .into();
        let fields = [
            ("some, fieldkey".into(), FieldValue::Boolean(false)),
            ("a=fieldkey".into(), FieldValue::Float(42.5)),
            ("otherfieldkey".into(), FieldValue::String("val".into())),
        ]
        .into();
        let data_point = DataPoint {
            measurement: "a, measurement".into(),
            tags,
            fields,
            timestamp: 8151561,
        };

        let expected = concat!(
            r#"a\,\ measurement,"#,
            r#"a\=tagkey=some\,\ tagvalue,othertagkey=otherval,some\,\ tagkey=a\=tagvalue "#,
            r#"a\=fieldkey=42.5,otherfieldkey="val",some\,\ fieldkey=false "#,
            "8151561"
        );

        assert_eq!(data_point.to_string(), expected);
    }
}
