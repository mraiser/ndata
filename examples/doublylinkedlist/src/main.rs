use ndata::init;
use ndata::dataarray::DataArray;

const VALUE:usize = 0;
const BEFORE:usize = 1;
const AFTER:usize = 2;

struct Node {
  data: DataArray,
}

impl Node{
  fn new(val:i64) -> Self {
    let mut a = DataArray::new();
    a.push_int(val);
    a.push_null();
    a.push_null();
    Node {
      data: a,
    }
  }
  
  fn from_array(a:DataArray) -> Self {
    Node {
      data: a,
    }
  }
  
  fn value(&self) -> i64 {
    self.data.get_int(VALUE)
  }
  
  fn previous(&self) -> Option<Node> {
    let before = self.data.get_property(BEFORE);
    if before.clone().is_null() { return None; }
    Some(Node::from_array(before.array()))
  }
  
  fn next(&self) -> Option<Node> {
    let after = self.data.get_property(AFTER);
    if after.clone().is_null() { return None; }
    Some(Node::from_array(after.array()))
  }
  
  fn first(&self) -> Node {
    let previous = self.previous();
    if previous.is_none() { return Node::from_array(self.data.clone()); }
    previous.unwrap().first()
  }
  
  fn last(&self) -> Node {
    let next = self.next();
    if next.is_none() { return Node::from_array(self.data.clone()); }
    next.unwrap().last()
  }
  
  fn insert_before(&mut self, val:i64) -> Self {
    let before = self.data.get_property(BEFORE);
    
    let mut a = DataArray::new();
    a.push_int(val);
    a.push_property(before.clone());
    a.push_array(self.data.clone());
    let node = Node {
      data: a,
    };
    
    if before.is_array(){
      let mut before = before.array();
      before.put_array(AFTER, node.data.clone());
    }
    
    self.data.put_array(BEFORE, node.data.clone());
    
    node
  }
  
  fn insert_after(&mut self, val:i64) -> Self {
    let after = self.data.get_property(AFTER);
    
    let mut a = DataArray::new();
    a.push_int(val);
    a.push_array(self.data.clone());
    a.push_property(after.clone());
    let node = Node {
      data: a,
    };
    
    if after.is_array(){
      let mut after = after.array();
      after.put_array(BEFORE, node.data.clone());
    }
    
    self.data.put_array(AFTER, node.data.clone());
    
    node
  }
  
  fn position(&self) -> usize {
    let before = self.data.get_property(BEFORE);
    if before.clone().is_null() { return 0; }
    let before = Node::from_array(before.array());
    before.position() + 1
  }
  
  fn length(&self) -> usize {
    let last = self.last();
    last.position()
  }
  
  fn to_vec(&self) -> Vec<i64> {
    let mut v = Vec::new();
    let mut node = self.first();
    loop {
      v.push(node.value());
      let next = node.next();
      if next.is_none() { break; }
      node = next.unwrap();
    }
    v
  }
}

fn main() {
  init();
  let mut node = Node::new(24);
  let mut node = node.insert_before(20);
  let mut node = node.insert_after(21);
  let mut node = node.insert_after(23);
  let node = node.insert_before(22);
  println!("current position: {}", node.position());
  println!("list length: {}", node.length());
  println!("first: {}", node.first().value());
  println!("last: {}", node.last().value());
  println!("all: {:?}", node.to_vec());
}
