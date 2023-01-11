use std::collections::BTreeMap;
use std::fmt;

use lazy_static::lazy_static;

use crate::mongodb::DataDocument;

use super::field_value::FieldValue;
use super::Replacer;

lazy_static! {
    static ref MEASUREMENT_REPLACER: Replacer = Replacer::new(&[",", " "], &[r#"\,"#, r#"\ "#]);
    static ref TAG_KV_FIELD_K_REPLACER: Replacer =
        Replacer::new(&[",", "=", " "], &[r#"\,"#, r#"\="#, r#"\ "#]);
}

#[derive(Debug)]
pub(crate) struct DataPointCreateError {
    pub doc_id: String,
    pub field: String,
    pub msg: &'static str,
}

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
        tags_age: &[String],
        timestamp: u64, // in seconds
    ) -> Result<Self, DataPointCreateError> {
        let mut fields: BTreeMap<String, FieldValue> = BTreeMap::new();
        for (key, value) in data_doc.data {
            let field_value = value.try_into().map_err(|msg| DataPointCreateError {
                doc_id: data_doc.id.to_owned(),
                field: key.to_owned(),
                msg,
            })?;
            fields.insert(key.to_owned(), field_value);
        }
        for key in tags_age {
            let source_timestamp: u64 = data_doc
                .source_timestamps
                .get(key)
                .ok_or(DataPointCreateError {
                    doc_id: data_doc.id.to_owned(),
                    field: key.to_owned(),
                    msg: "missing source timestamp for field",
                })?
                .as_datetime()
                .ok_or(DataPointCreateError {
                    doc_id: data_doc.id.to_owned(),
                    field: key.to_owned(),
                    msg: "invalid datetime",
                })?
                .timestamp_millis()
                .try_into()
                .map_err(|_| DataPointCreateError {
                    doc_id: data_doc.id.to_owned(),
                    field: key.to_owned(),
                    msg: "timestamp out of range",
                })?;
            let value = FieldValue::UInteger(timestamp - source_timestamp / 1000);
            fields.insert(format!("{}Age", key), value);
        }

        let tags = [("id".into(), data_doc.id)].into();

        Ok(Self {
            measurement,
            tags,
            fields,
            timestamp,
        })
    }
}

impl fmt::Display for DataPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let escaped_measurement = MEASUREMENT_REPLACER.replace_all(&self.measurement);
        f.write_str(&escaped_measurement)?;

        for (key, value) in &self.tags {
            f.write_str(",")?;
            let escaped_key = TAG_KV_FIELD_K_REPLACER.replace_all(key);
            f.write_str(&escaped_key)?;
            f.write_str("=")?;
            let escaped_value = TAG_KV_FIELD_K_REPLACER.replace_all(value);
            f.write_str(&escaped_value)?;
        }

        f.write_str(" ")?;

        let mut fields = self.fields.iter().peekable();
        while let Some((key, value)) = fields.next() {
            let escaped_key = TAG_KV_FIELD_K_REPLACER.replace_all(key);
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
    use mongodb::bson::{self, doc, DateTime};

    use crate::mongodb::DataDocument;

    use super::*;

    #[test]
    fn create() {
        let now_secs = DateTime::now().timestamp_millis() / 1000;
        let document = doc! {
            "_id": "anid",
            "updatedSince": 0,
            "data": {
                "first": true,
                "second": "a_string",
                "third": 37.5,
                "fourth": 42,
            },
            "sourceTimestamps": {
                "some": DateTime::from_millis((now_secs - 42) * 1000),
                "other": DateTime::from_millis((now_secs - 3600) * 1000),
                "notRelevant": DateTime::from_millis((now_secs - 5) * 1000),
            },
        };
        let data_document: DataDocument = bson::from_document(document).unwrap();
        let measurement = String::from("some_measurement");
        let tags_validity_time = &["some".to_string(), "other".to_string()];

        let data_point = DataPoint::create(
            data_document,
            measurement,
            tags_validity_time,
            now_secs as u64,
        )
        .unwrap();

        assert_eq!(data_point.measurement, "some_measurement");
        assert_eq!(data_point.tags["id"], "anid");
        assert_eq!(data_point.fields["first"], FieldValue::Boolean(true));
        assert_eq!(
            data_point.fields["second"],
            FieldValue::String("a_string".into())
        );
        assert_eq!(data_point.fields["third"], FieldValue::Float(37.5));
        assert_eq!(data_point.fields["fourth"], FieldValue::Integer(42));
        assert_eq!(data_point.fields["someAge"], FieldValue::UInteger(42));
        assert_eq!(data_point.fields["otherAge"], FieldValue::UInteger(3600));
        assert!(data_point.fields.get("notRelevantAge").is_none());
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
