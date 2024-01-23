use ndata::init;
use ndata::mirror;
use ndata::dataobject::DataObject;
use ndata::NDataConfig;
use std::env;
use std::process::Command;

fn main() {
  let args: Vec<String> = env::args().collect();
  if args.len() == 1 {
    println!("NO ARGS");
    
    let x = init();
    println!("hey {}", x.to_string());    
    let mut o = DataObject::new();
    o.put_string("xxx", "yyy");
    
    let args = vec![x.to_string()];
    let cmd = Command::new("./target/debug/multiprocess")
      .args(args)
      .spawn()
      .unwrap()
      .wait_with_output()
      .unwrap();
    println!("FINISHED {:?}", cmd);
  }
  else {
    println!("THERE BE ARGS");
    let x = NDataConfig::from_string(args[1].to_owned());
    println!("ho {}", x.to_string());
    mirror(x);
    let x = DataObject::get(0);
    println!("hey {}", x.to_string());
    println!("DONE");
  }
}
