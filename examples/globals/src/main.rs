use ndata;
use ndata::dataobject::DataObject;

fn main() {
  init();
  do_a_thing();
  print_result();
}

fn init() {
  ndata::init();
  DataObject::new().incr();
}

fn globals() -> DataObject {
  DataObject::get(0)
}

fn do_a_thing() {
  globals().put_string("result", "Hello, world!");
}

fn print_result() {
  println!("{}", globals().get_string("result"));
}
