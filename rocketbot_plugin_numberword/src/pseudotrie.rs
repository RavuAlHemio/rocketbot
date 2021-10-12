use std::collections::HashMap;
use std::hash::Hash;


#[derive(Clone, Debug)]
pub struct Pseudotrie<K: Eq + Hash, V> {
    root_node: Node<K, V>,
}
impl<K: Eq + Hash, V> Default for Pseudotrie<K, V> {
    fn default() -> Self {
        let root_node: Node<K, V> = Node {
            leaf_value: None,
            children: HashMap::new(),
        };
        Self {
            root_node,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct Node<K: Eq + Hash, V> {
    leaf_value: Option<V>,
    children: HashMap<K, Node<K, V>>,
}

impl<K: Eq + Hash, V> Pseudotrie<K, V> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn insert(&mut self, mut key: Vec<K>, value: V) -> Option<V> {
        let mut cur_node = &mut self.root_node;
        for k in key.drain(..) {
            let child_node = cur_node.children.entry(k)
                .or_insert_with(|| Node {
                    leaf_value: None,
                    children: HashMap::new(),
                });
            cur_node = child_node;
        }
        std::mem::replace(&mut cur_node.leaf_value, Some(value))
    }

    pub fn remove(&mut self, key: &[K]) -> Option<V> {
        let mut cur_node = &mut self.root_node;
        for k in key {
            let child_node = match cur_node.children.get_mut(k) {
                None => return None,
                Some(cn) => cn,
            };
            cur_node = child_node;
        }
        let old_val = std::mem::replace(&mut cur_node.leaf_value, None);

        self.cleanup();

        old_val
    }

    fn cleanup_node(node: &mut Node<K, V>) {
        // have children clean up first
        for child in node.children.values_mut() {
            Self::cleanup_node(child);
        }

        node.children.retain(|_k, v|
            if v.leaf_value.is_some() {
                // we still need this node; it contains a value
                true
            } else if v.children.len() > 0 {
                // we still need this node; it contains children
                true
            } else {
                // begone
                false
            }
        );
    }

    fn cleanup(&mut self) {
        Self::cleanup_node(&mut self.root_node);
    }

    pub fn get(&self, key: &[K]) -> Option<&V> {
        let mut cur_node = &self.root_node;
        for k in key {
            let child_node = match cur_node.children.get(k) {
                None => return None,
                Some(cn) => cn,
            };
            cur_node = child_node;
        }
        cur_node.leaf_value.as_ref()
    }

    pub fn contains_entries_with_prefix(&self, prefix: &[K]) -> bool {
        let mut cur_node = &self.root_node;
        for k in prefix {
            let child_node = match cur_node.children.get(k) {
                None => return false,
                Some(cn) => cn,
            };
            cur_node = child_node;
        }
        true
    }

    pub fn len(&self) -> usize {
        let mut node_stack = Vec::new();
        node_stack.push(&self.root_node);

        let mut count = 0;
        while let Some(node) = node_stack.pop() {
            if node.leaf_value.is_some() {
                count += 1;
            }

            for child in node.children.values() {
                node_stack.push(child);
            }
        }

        count
    }

    pub fn to_vec(&self) -> Vec<(Vec<&K>, &V)> {
        let mut values = Vec::new();

        let mut node_stack = Vec::new();
        node_stack.push((Vec::new(), &self.root_node));

        while let Some((prefix, node)) = node_stack.pop() {
            if let Some(val) = node.leaf_value.as_ref() {
                values.push((prefix.clone(), val));
            }

            for (key, child) in &node.children {
                let mut sub_prefix = prefix.clone();
                sub_prefix.push(key);
                node_stack.push((sub_prefix, child));
            }
        }

        values
    }
}

#[derive(Clone, Debug)]
pub struct StringPseudotrie<V> {
    pseudotrie: Pseudotrie<char, V>,
}
impl<V> StringPseudotrie<V> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn insert(&mut self, key: &str, value: V) -> Option<V> {
        let key_chars: Vec<char> = key.chars().collect();
        self.pseudotrie.insert(key_chars, value)
    }

    pub fn remove(&mut self, key: &str) -> Option<V> {
        let key_chars: Vec<char> = key.chars().collect();
        self.pseudotrie.remove(&key_chars)
    }

    pub fn get(&self, key: &str) -> Option<&V> {
        let key_chars: Vec<char> = key.chars().collect();
        self.pseudotrie.get(&key_chars)
    }

    pub fn contains_entries_with_prefix(&self, prefix: &str) -> bool {
        let prefix_chars: Vec<char> = prefix.chars().collect();
        self.pseudotrie.contains_entries_with_prefix(&prefix_chars)
    }

    pub fn len(&self) -> usize {
        self.pseudotrie.len()
    }

    pub fn to_vec(&self) -> Vec<(String, &V)> {
        self.pseudotrie.to_vec()
            .iter()
            .map(|(key, value)|
                (
                    key.iter().map(|c| *c).collect(),
                    *value,
                )
            )
            .collect()
    }
}
impl<V> Default for StringPseudotrie<V> {
    fn default() -> Self {
        Self { pseudotrie: Default::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::StringPseudotrie;

    #[test]
    fn test_empty() {
        let ptrie: StringPseudotrie<usize> = StringPseudotrie::new();
        assert_eq!(0, ptrie.len());
        assert_eq!(0, ptrie.to_vec().len());
        assert!(ptrie.contains_entries_with_prefix(""));
        assert!(!ptrie.contains_entries_with_prefix("a"));
        assert!(!ptrie.contains_entries_with_prefix("b"));
    }

    #[test]
    fn test_a_few() {
        let mut ptrie: StringPseudotrie<usize> = StringPseudotrie::new();
        ptrie.insert("a", 0);
        ptrie.insert("an", 10);
        ptrie.insert("and", 20);
        ptrie.insert("banana", 30);

        assert_eq!(4, ptrie.len());
        assert!(ptrie.contains_entries_with_prefix(""));
        assert!(ptrie.contains_entries_with_prefix("a"));
        assert!(ptrie.contains_entries_with_prefix("an"));
        assert!(ptrie.contains_entries_with_prefix("and"));
        assert!(!ptrie.contains_entries_with_prefix("ax"));
        assert!(ptrie.contains_entries_with_prefix("b"));
        assert!(ptrie.contains_entries_with_prefix("ba"));
        assert!(ptrie.contains_entries_with_prefix("banan"));
        assert!(ptrie.contains_entries_with_prefix("banana"));
        assert!(!ptrie.contains_entries_with_prefix("bananas"));
        assert!(!ptrie.contains_entries_with_prefix("banane"));
        assert!(!ptrie.contains_entries_with_prefix("x"));

        let ptrie_vec = ptrie.to_vec();
        assert_eq!(4, ptrie_vec.len());
        assert!(ptrie_vec.contains(&("a".to_owned(), &0)));
        assert!(ptrie_vec.contains(&("an".to_owned(), &10)));
        assert!(ptrie_vec.contains(&("and".to_owned(), &20)));
        assert!(ptrie_vec.contains(&("banana".to_owned(), &30)));
    }
}
