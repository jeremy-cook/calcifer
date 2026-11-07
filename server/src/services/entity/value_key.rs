use calcifer::v1::property_value;

/// A property value's `properties.value_key` (ADR 9): canonical text that sorts
/// as the values do, so SQL can filter and order by it. A date is its ISO day, a
/// select its option key and a text itself; a number is `number_key`. A
/// relation value has none. Every write of `value_blob` writes this beside it.
pub(crate) fn value_key(value: Option<&property_value::Value>) -> Option<String> {
    match value? {
        property_value::Value::Date(v)
        | property_value::Value::Select(v)
        | property_value::Value::Text(v) => Some(v.clone()),
        property_value::Value::Number(n) => Some(number_key(*n)),
        property_value::Value::Relation(_) | property_value::Value::Relations(_) => None,
    }
}

/// Sixteen hex digits that sort as the numbers do: the IEEE 754 bits with the
/// sign bit flipped for a positive number and every bit flipped for a negative
/// one. -0 is written as 0, so the two are equal.
fn number_key(n: f64) -> String {
    let n = if n == 0.0 { 0.0 } else { n };
    let bits = n.to_bits();
    let ordered = if bits >> 63 == 1 {
        !bits
    } else {
        bits | 1 << 63
    };
    format!("{ordered:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use calcifer::v1::{EntityRef, EntityRefList};

    #[test]
    fn scalar_values_have_keys() {
        let date = property_value::Value::Date("2026-11-07".into());
        assert_eq!(value_key(Some(&date)).as_deref(), Some("2026-11-07"));
        let select = property_value::Value::Select("open".into());
        assert_eq!(value_key(Some(&select)).as_deref(), Some("open"));
        let text = property_value::Value::Text("Apollo".into());
        assert_eq!(value_key(Some(&text)).as_deref(), Some("Apollo"));
        assert_eq!(value_key(None), None);
    }

    #[test]
    fn relation_values_have_none() {
        let one = property_value::Value::Relation(EntityRef::default());
        let many = property_value::Value::Relations(EntityRefList::default());
        assert_eq!(value_key(Some(&one)), None);
        assert_eq!(value_key(Some(&many)), None);
    }

    #[test]
    fn number_keys_sort_as_numbers() {
        let numbers = [
            f64::NEG_INFINITY,
            -1e9,
            -2.5,
            -1.0,
            -0.001,
            0.0,
            0.001,
            1.0,
            2.5,
            10.0,
            1e9,
            f64::INFINITY,
        ];
        let keys: Vec<String> = numbers.iter().map(|&n| number_key(n)).collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
        assert_eq!(number_key(-0.0), number_key(0.0));
    }
}
