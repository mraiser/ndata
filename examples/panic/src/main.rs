use ndata::init;
use std::panic;
use ndata::dataobject::DataObject;

fn main() {
  init();
  
  let mut o = DataObject::new();
  o.put_int("arg1", 42);
  o.put_int("arg2", 0);
  
  let data_ref = o.data_ref;
  let result = panic::catch_unwind(|| {
    let mut oo = DataObject::get(data_ref);
    let x = oo.get_int("arg1");
    let y = oo.get_int("arg2");
    oo.put_int("sum", x+y);
    oo.put_int("difference", x-y);
    oo.put_int("product", x*y);
    oo.put_int("quotient", x/y);
  });
  
  match result {
    Ok(_x) => (), // Not gonna happen
    Err(_x) => {
      println!("Ooops, there was a panic!");
    }
  }
  
  println!("RESULT: {}", o.to_string());
}
