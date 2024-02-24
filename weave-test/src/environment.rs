use std::collections::HashMap;
use std::hash::Hash;

pub trait HashMapExt<K: Hash + Eq + Clone, V: Clone> {
    fn append(&mut self, other: &HashMap<K, V>);
    fn try_append(&mut self, other: &Option<HashMap<K,V>>);
}

impl <K: Hash + Eq + Clone,V: Clone> HashMapExt<K,V> for HashMap<K,V> {
    fn append(&mut self, other: &HashMap<K,V>) {
        for (k, v) in other.iter() {
            if self.contains_key(k) {
                self.remove(k);
            }

            self.insert((*k).clone(), (*v).clone());
        }
    }

    fn try_append(&mut self, other: &Option<HashMap<K, V>>) {
        if let Some(other) = other{
            self.append(other);
        }
    }
}