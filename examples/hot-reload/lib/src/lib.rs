use ndata::mirror as mirror2;
use ndata::NDataConfig;
use ndata::dataobject::DataObject;

#[no_mangle]
pub fn mirror(config: NDataConfig) {
  mirror2(config);
}

#[no_mangle]
pub fn step(data_ref: usize) {
  let mut o = DataObject::get(data_ref);
  let mut i = o.get_int("counter");
  i += 1;
  o.put_int("counter", i);
}
