use ndata::*;
use ndata::dataobject::*;
use ndata::dataarray::*;
use ndata::databytes::*;

fn main() {
  init();
  
  {
    let mut o = DataObject::new();
    o.put_array("a", DataArray::new());
    o.put_bytes("b", DataBytes::from_bytes(&"Hello, world!".as_bytes().to_vec()));
    
    println!("{}", o.to_string());
    print_heap();
    
    o.put_int("a", 42);
    o.put_string("b", "Hello world!");
    
    println!("{}", o.to_string());
    gc();
    print_heap();
  }
  
  println!("DONE");
  gc();
  print_heap();
}
