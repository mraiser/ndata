use ndata::init;
use std::thread;
use ndata::dataobject::DataObject;

fn main() {
  init();
  
  let mut o = DataObject::new();
  o.put_string("arg", "Hello");
  o.put_boolean("done", false);
  
  let mut oo = o.clone();
  let _t = thread::spawn(move || {
    let s = oo.get_string("arg") + ", world!";
    oo.put_string("result", &s);
    oo.put_boolean("done", true);
  });

  while !o.get_boolean("done") {}
  
  println!("{}", o.get_string("result"));
}
