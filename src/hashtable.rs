use std::cell::RefCell;
use std::rc::Rc;

/// A node in the hash map.
pub struct HNode {
    next: Option<Rc<RefCell<HNode>>>,
    hcode: u64,

}


impl  HNode {
    /// Creates a new `HNode` with the given `hcode`.
    ///
    /// # Example
    ///
    /// ```
    /// let node = HNode::new(42);
    /// ```
    pub fn new(hcode: u64) -> Link {
        Rc::new(RefCell::new(HNode {
            next: None,
            hcode: hcode,
          

        }))
    }
}

/// A hash map with two tables for resizing.
pub struct HMap {
    ht1: Option<HTab>,
    ht2: Option<HTab>,
    resizing_pos: usize,
}

const K_MAX_LOAD_FACTOR: usize = 8;
const K_RESIZING_WORK: usize = 128;

impl HMap {
    /// Creates a new, empty `HMap`.
    ///
    /// # Example
    ///
    /// ```
    /// let map = HMap::new();
    /// ```
    pub fn new() -> HMap {
        HMap {
            ht1: None,
            ht2: None,
            resizing_pos: 0,
        }
    }

    /// Inserts a new node into the hash map.
    ///
    /// # Example
    ///
    /// ```
    /// let mut map = HMap::new();
    /// let node = HNode::new(42);
    /// map.hm_insert(node);
    /// ```
    pub fn hm_insert(&mut self, node: Link) {
        if self.ht1.is_none() {
            self.ht1 = Some(HTab::new(4));
        }
        let htab = self.ht1.as_mut().unwrap();
        htab.insert(node);

        if self.ht2.is_none() {
            let load_factor = self.ht1.as_ref().unwrap().size / (self.ht1.as_ref().unwrap().mask + 1);
            if load_factor > K_MAX_LOAD_FACTOR {
                self.start_resizing();
            }
        }
        self.help_resizing();
    }

    /// Starts the resizing process by creating a new table and moving nodes from the old table.
    fn start_resizing(&mut self) {
        assert!(self.ht2.is_none());
        self.ht2 = self.ht1.take();
        self.ht1 = Some(HTab::new((self.ht2.as_ref().unwrap().mask + 1) * 2));
        self.resizing_pos = 0;
    }

    /// Helps the resizing process by moving nodes from the old table to the new table.
    fn help_resizing(&mut self) {
        let mut nwork = 0;
        let tab2 = self.ht2.as_mut().unwrap();
        let tab1 = self.ht1.as_mut().unwrap();
        while nwork < K_RESIZING_WORK && tab2.size > 0 {
            let from = tab2.table[self.resizing_pos].take();
            if from.is_none() {
                self.resizing_pos += 1;
                continue;
            }
            tab1.insert(from.expect("wtf happen?"));
            nwork += 1;
        }

        if tab2.size == 0 && self.ht2.is_some() {
            let _ = self.ht2.take();
        }
    }


    fn hm_lookup(&mut self,node:Link,comparator:Comparator)->Option<Link>{
        self.help_resizing();
        let mut from=self.ht1.as_ref().unwrap().h_lookup(node.clone(), comparator);
        if from.is_none(){
            from=self.ht2.as_ref().unwrap().h_lookup(node.clone(), comparator)
        };
        return from;

    }
}

/// A hash table with a vector of nodes.
struct HTab {
    table: Vec<Option<Link>>,
    mask: usize,
    size: usize,
}

pub type Link = Rc<RefCell<HNode>>;
type Comparator=fn(Link, Link) -> bool ;



impl HTab {
    /// Creates a new `HTab` with the given `size`.
    ///
    /// # Example
    ///
    /// ```
    /// let tab = HTab::new(4);
    /// ```
    fn new(size: usize) -> Self {
        assert!(size > 0 && ((size - 1) & size) == 0);
        HTab {
            table: vec![None; size],
            mask: size - 1,
            size: 0,
        }
    }

    /// Inserts a new node into the hash table.
    ///
    /// # Example
    ///
    /// ```
    /// let mut tab = HTab::new(4);
    /// let node = HNode::new(42);
    /// tab.insert(node);
    /// ```
    fn insert(&mut self, node: Link) {
        let hcode = node.borrow().hcode;
        let pos = (hcode as usize) & self.mask;
        let prev = self.table[pos].take();
        node.borrow_mut().next = prev;
        self.table[pos] = Some(node);
        self.size += 1;
    }

    /// Looks up a node in the hash table using a custom comparator.
    ///
    ///# Example
    ///
    /// ```
    /// let mut tab = HTab::new(4);
    /// let node = HNode::new(42);
    /// tab.insert(node.clone());
    /// let found = tab.h_lookup(node, |a, b| a.borrow().hcode == b.borrow().hcode);
    /// assert!(found.is_some());
    /// ```
    fn h_lookup(&self, node: Link, comparator: Comparator)-> Option<Link> {
        let hcode = node.borrow().hcode;
        let pos = (hcode as usize) & self.mask;
        let mut cur = self.table[pos].clone();
        while let Some(ref cur_node) = cur {
            if comparator(node.clone(), cur_node.clone()) {
                return Some(cur_node.clone());
            }
        }
        return None;
    }

    /// Detaches a node from the hash table.
    ///
    /// # Example
    ///
    /// ```
    /// let mut tab = HTab::new(4);
    /// let node = HNode::new(42);
    /// tab.insert(node.clone());
    /// let mut from = Some(node.clone());
    /// let detached = tab.h_detach(&mut from);
    /// assert!(detached.is_some());
    /// ```
    fn h_detach(&mut self, from: &mut Option<Rc<RefCell<HNode>>>) -> Option<Rc<RefCell<HNode>>> {
        let node = from.take()?; // Take ownership of the node if it exists
        *from = node.borrow().next.clone(); // Update 'from' to point to the next node
        self.size -= 1; // Decrease the size of the hash table
        Some(node) // Return the detached node
    }
}


