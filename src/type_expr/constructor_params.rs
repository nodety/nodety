//! Insertion-ordered constructor parameters (`Map<K, V>` args and record fields).
//!
//! Iteration follows insertion order. [`PartialEq`], [`Eq`] and [`Hash`] ignore order, matching
//! the previous [`BTreeMap`](std::collections::BTreeMap) identity for records: `{b: T, a: U}` is
//! the same type as `{a: U, b: T}`.
use crate::{
    Type,
    type_expr::{ParamRef, TypeExpr, TypeRef},
};
use std::{
    collections::BTreeMap,
    hash::{Hash, Hasher},
    slice, vec,
};

#[cfg(feature = "json-schema")]
use schemars::JsonSchema;
#[cfg(feature = "serde")]
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{MapAccess, Visitor},
    ser::SerializeMap,
};
#[cfg(feature = "tsify")]
use tsify::Tsify;

/// Named type arguments of a [`TypeExpr::Constructor`](crate::TypeExpr::Constructor).
///
/// Duplicate keys last-win in place: the original insertion position is kept.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "tsify", derive(Tsify))]
#[cfg_attr(feature = "tsify", tsify(type = "Record<string, TypeExpr<T, R>>"))]
pub struct ConstructorParams<T: Type, R: TypeRef = ParamRef>(Vec<(String, TypeExpr<T, R>)>);

impl<T: Type, R: TypeRef> ConstructorParams<T, R> {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn get(&self, key: impl AsRef<str>) -> Option<&TypeExpr<T, R>> {
        let key = key.as_ref();
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn get_mut(&mut self, key: impl AsRef<str>) -> Option<&mut TypeExpr<T, R>> {
        let key = key.as_ref();
        self.0.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn contains_key(&self, key: impl AsRef<str>) -> bool {
        self.get(key).is_some()
    }

    /// Inserts `value` at `key`. If the key already exists, the value is replaced and the original
    /// insertion position is kept.
    pub fn insert(&mut self, key: impl Into<String>, value: TypeExpr<T, R>) -> Option<TypeExpr<T, R>> {
        let key = key.into();
        if let Some((_, existing)) = self.0.iter_mut().find(|(k, _)| k == &key) {
            return Some(std::mem::replace(existing, value));
        }
        self.0.push((key, value));
        None
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.0.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl Iterator<Item = &TypeExpr<T, R>> {
        self.0.iter().map(|(_, v)| v)
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut TypeExpr<T, R>> {
        self.0.iter_mut().map(|(_, v)| v)
    }

    pub fn iter(&self) -> slice::Iter<'_, (String, TypeExpr<T, R>)> {
        self.0.iter()
    }

    pub fn iter_mut(&mut self) -> slice::IterMut<'_, (String, TypeExpr<T, R>)> {
        self.0.iter_mut()
    }
}

impl<T: Type, R: TypeRef> Default for ConstructorParams<T, R> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Type, R: TypeRef> PartialEq for ConstructorParams<T, R> {
    fn eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len() && self.0.iter().all(|(k, v)| other.get(k) == Some(v))
    }
}

impl<T: Type + Eq, R: TypeRef + Eq> Eq for ConstructorParams<T, R> where T::Operator: Eq {}

impl<T: Type + Hash, R: TypeRef + Hash> Hash for ConstructorParams<T, R>
where
    T::Operator: Hash,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        let mut entries: Vec<_> = self.0.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries.hash(state);
    }
}

impl<T: Type, R: TypeRef> FromIterator<(String, TypeExpr<T, R>)> for ConstructorParams<T, R> {
    fn from_iter<I: IntoIterator<Item = (String, TypeExpr<T, R>)>>(iter: I) -> Self {
        let mut params = Self::new();
        for (k, v) in iter {
            params.insert(k, v);
        }
        params
    }
}

impl<T: Type, R: TypeRef> Extend<(String, TypeExpr<T, R>)> for ConstructorParams<T, R> {
    fn extend<I: IntoIterator<Item = (String, TypeExpr<T, R>)>>(&mut self, iter: I) {
        for (k, v) in iter {
            self.insert(k, v);
        }
    }
}

impl<T: Type, R: TypeRef, const N: usize> From<[(String, TypeExpr<T, R>); N]> for ConstructorParams<T, R> {
    fn from(arr: [(String, TypeExpr<T, R>); N]) -> Self {
        arr.into_iter().collect()
    }
}

impl<T: Type, R: TypeRef> From<BTreeMap<String, TypeExpr<T, R>>> for ConstructorParams<T, R> {
    fn from(map: BTreeMap<String, TypeExpr<T, R>>) -> Self {
        map.into_iter().collect()
    }
}

impl<T: Type, R: TypeRef> IntoIterator for ConstructorParams<T, R> {
    type Item = (String, TypeExpr<T, R>);
    type IntoIter = vec::IntoIter<(String, TypeExpr<T, R>)>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, T: Type, R: TypeRef> IntoIterator for &'a ConstructorParams<T, R> {
    type Item = &'a (String, TypeExpr<T, R>);
    type IntoIter = slice::Iter<'a, (String, TypeExpr<T, R>)>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a, T: Type, R: TypeRef> IntoIterator for &'a mut ConstructorParams<T, R> {
    type Item = &'a mut (String, TypeExpr<T, R>);
    type IntoIter = slice::IterMut<'a, (String, TypeExpr<T, R>)>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

#[cfg(feature = "serde")]
impl<T, R> Serialize for ConstructorParams<T, R>
where
    T: Type + Serialize,
    T::Operator: Serialize,
    R: TypeRef + Serialize,
{
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (k, v) in &self.0 {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

#[cfg(feature = "serde")]
impl<'de, T, R> Deserialize<'de> for ConstructorParams<T, R>
where
    T: Type + Deserialize<'de>,
    T::Operator: Deserialize<'de>,
    R: TypeRef + Deserialize<'de>,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct MapVisitor<T: Type, R: TypeRef> {
            _marker: std::marker::PhantomData<(T, R)>,
        }

        impl<'de, T, R> Visitor<'de> for MapVisitor<T, R>
        where
            T: Type + Deserialize<'de>,
            T::Operator: Deserialize<'de>,
            R: TypeRef + Deserialize<'de>,
        {
            type Value = ConstructorParams<T, R>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a map of constructor parameters")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut params = ConstructorParams::new();
                while let Some((k, v)) = map.next_entry::<String, TypeExpr<T, R>>()? {
                    params.insert(k, v);
                }
                Ok(params)
            }
        }

        deserializer.deserialize_map(MapVisitor { _marker: std::marker::PhantomData })
    }
}

#[cfg(feature = "json-schema")]
impl<T, R> JsonSchema for ConstructorParams<T, R>
where
    T: Type + JsonSchema,
    T::Operator: JsonSchema,
    R: TypeRef + JsonSchema,
{
    fn schema_name() -> std::borrow::Cow<'static, str> {
        BTreeMap::<String, TypeExpr<T, R>>::schema_name()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        BTreeMap::<String, TypeExpr<T, R>>::json_schema(generator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NoOperator;
    use std::collections::HashSet;

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    enum Atom {
        A,
    }

    impl Type for Atom {
        type Operator = NoOperator;
    }

    fn ty() -> TypeExpr<Atom> {
        TypeExpr::Type(Atom::A)
    }

    #[test]
    fn insertion_order() {
        let mut params = ConstructorParams::new();
        params.insert("z", ty());
        params.insert("a", ty());
        assert_eq!(params.keys().cloned().collect::<Vec<_>>(), ["z", "a"]);
    }

    #[test]
    fn insert_overwrite_keeps_position() {
        let mut params = ConstructorParams::new();
        params.insert("z", ty());
        params.insert("a", ty());
        params.insert("z", TypeExpr::Any);
        assert_eq!(params.keys().cloned().collect::<Vec<_>>(), ["z", "a"]);
        assert_eq!(params.get("z"), Some(&TypeExpr::Any));
    }

    #[test]
    fn eq_ignores_order() {
        let a: ConstructorParams<Atom> = [("z".into(), ty()), ("a".into(), ty())].into();
        let b: ConstructorParams<Atom> = [("a".into(), ty()), ("z".into(), ty())].into();
        assert_eq!(a, b);
    }

    #[test]
    fn hash_ignores_order() {
        let a: ConstructorParams<Atom> = [("z".into(), ty()), ("a".into(), ty())].into();
        let b: ConstructorParams<Atom> = [("a".into(), ty()), ("z".into(), ty())].into();
        let mut set = HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
    }
}
