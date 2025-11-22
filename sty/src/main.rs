
use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;
use std::any::type_name_of_val;

use std::env;



fn main() {

    let args: Vec<String> = env::args().collect();
    // dbg!(args);

    if args.len() != 3 {
      println!("I expect <column id> <data filename> as arguments");
      std::process::exit(1)
    }

    let colid: usize = args[1].parse().expect("Failed to parse string to integer");

    let thefname = &args[2];
    let fopen    = read_lines(thefname);

      // Result<std::io::Lines<BufReader<File>>, std::io::Error>

    let mut lines_iter: io::Lines<io::BufReader<File>> = match fopen {
       Err(e) => {
          eprintln!("Error {:?}", e); // Use {:?} for full error info
          std::process::exit(1);
        },
        Ok(it) => it
    };

    let myline = lines_iter.next();
    println!("myline {}", type_name_of_val(&myline));

    let val: String = match myline {
      Some(Ok(val)) => { val },
      Some(Err(e)) => panic!("Mmm error reading first line: {:?}", e),
      None => panic!("No line was obtained"),
    };

    println!("Header line: {}", val);

    println!("TNV {}", type_name_of_val(&val));

    let v2 = lines_iter.next().expect("second line").expect("second value");
    println!("v2 {}", type_name_of_val(&v2));
    println!("result {}", v2);

    // let val3 = val.expect("oops not Ok 3");
    // println!("val3 {}", type_name_of_val(&val3));

    // if let Some(h) = myline {
    //   match h {
    //     Ok(v) => {
    //       println!("result is {}", v);
    //       println!("{}", type_name_of_val(&v));
    //     }
    //     Err(e) => {
    //       println!("error: {}", e);
    //       std::process::exit(1)
    //     }
    //   }
    // }

    // Consumes the iterator, returns an (Optional) String
    for line in lines_iter.map_while(Result::ok) {
        let fields: Vec<&str> = line.split('\t').collect();
        println!("Column value is {}", fields[colid]);
    }

    println!("Done processing");
}

// The output is wrapped in a Result to allow matching on errors.
// Returns an Iterator to the Reader of the lines of the file.
fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where P: AsRef<Path>, {
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}




