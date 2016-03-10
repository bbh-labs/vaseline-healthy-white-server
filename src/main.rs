// Standard Library
use std::env::current_dir;
use std::fs;
use std::fs::metadata;
use std::path::Path;
use std::process::Command;
use std::os::unix::fs::symlink;

// Iron
extern crate iron;
extern crate mount;
extern crate persistent;
extern crate router;
extern crate staticfile;
extern crate urlencoded;
use iron::prelude::*;
use iron::status;
use iron::typemap::Key;
use mount::Mount;
use persistent::Write;
use router::Router;
use staticfile::Static;
use urlencoded::UrlEncodedBody;

// Serialize
extern crate rustc_serialize;
use rustc_serialize::json;

const ADDRESS: &'static str = "localhost:8080";

#[derive(Copy, Clone)]
pub struct LastRawOutput;
impl Key for LastRawOutput {
    type Value = String;
}

#[derive(Copy, Clone)]
pub struct LastOutput;
impl Key for LastOutput {
    type Value = String;
}

#[derive(Copy, Clone)]
pub struct LastFinalOutput;
impl Key for LastFinalOutput {
    type Value = String;
}

fn file_exists(filename: &str) -> bool {
    match metadata(filename) {
        Ok(_) => true,
        Err(_) => false,
    }
}

fn available_filename(prefix: &str, suffix: &str) -> String {
    for i in 0..999999 {
        let filename = format!("{}{}{}", prefix, i, suffix);
        if !file_exists(&filename) {
            return filename.to_string();
        }
    }

    "".to_string()
}

fn find_last_output(prefix: &str, suffix: &str) -> String {
    let mut result = String::from("");

    for i in 0..999999 {
        let filename = format!("{}{}{}", prefix, i, suffix);
        if !file_exists(&filename) {
            return result;
        }
        result = filename
    }

    result
}

fn result_handler(req: &mut Request) -> IronResult<Response> {
    let mutex = req.get::<Write<LastFinalOutput>>().unwrap();
    let last_final_output = mutex.lock().unwrap();
    Ok(Response::with((status::Ok, last_final_output.clone())))
}

fn capture_handler(req: &mut Request) -> IronResult<Response> {
	let response;

	if let Some(output_filename) = capture("images/raw_output", ".jpg") {
		let mutex = req.get::<Write<LastRawOutput>>().unwrap();
		let mut last_output = mutex.lock().unwrap();
		*last_output = output_filename.clone();
		response = Response::with((status::Ok, output_filename.clone()));
	} else {
		response = Response::with(status::InternalServerError);
	}

	Ok(response)
}

fn capture(output_prefix: &str, output_suffix: &str) -> Option<String> {
    let output_filename = available_filename(output_prefix, output_suffix);
    let output = Command::new("gphoto2")
                     .arg("--auto-detect")
                     .arg("--capture-image-and-download")
                     .arg("--filename")
                     .arg(&output_filename)
                     .output()
                     .unwrap_or_else(|e| panic!("failed to execute process: {}", e));

    println!("Raw output filename: {}", &output_filename);

	let option;
    let output_message = if output.status.success() {
		option = Some(output_filename);
        output.stdout
    } else {
		option = None;
        output.stderr
    };

	if let Ok(message) = String::from_utf8(output_message) {
		println!("{}", message);
	}

	option
}

fn post_process_handler(req: &mut Request) -> IronResult<Response> {
	// Get Last Raw Output
    let mutex = req.get::<Write<LastRawOutput>>().unwrap();
    let last_raw_output = mutex.lock().unwrap();

	// Do Post Processing
	let abc = vec!["a", "b", "c"];
	let mut output_filenames = Vec::<String>::new();

	for i in 0..abc.len() {
		if let Some(output_filename) = post_process(&last_raw_output, &format!("style_{}.xmp", abc[i]), &format!("images/output_{}", abc[i]), ".jpg") {
			// Set Last Output
			let mutex = req.get::<Write<LastOutput>>().unwrap();
			let mut last_output = mutex.lock().unwrap();
			*last_output = output_filename.clone();
			output_filenames.push(output_filename.clone());
		} else {
			return Ok(Response::with(status::InternalServerError));
		}
	}

	if let Ok(s) = json::encode(&output_filenames) {
		return Ok(Response::with((status::Ok, s)));
	} else {
		return Ok(Response::with(status::InternalServerError));
	}
}

fn post_process(input_filename: &str, style_filename: &str, output_prefix: &str, output_suffix: &str) -> Option<String> {
    let output_filename = available_filename(output_prefix, output_suffix);
    let output = Command::new("darktable-cli")
                     .arg(input_filename)
                     .arg(style_filename)
                     .arg(&output_filename)
                     .output()
                     .unwrap_or_else(|e| panic!("failed to execute process: {}", e));
    
	println!("Output filename: {}", &output_filename);

	let option;
    let output_message = if output.status.success() {
		option = Some(output_filename);
		output.stdout
	} else {
		option = None;
		output.stderr
	};

	if let Ok(message) = String::from_utf8(output_message) {
		println!("{}", message);
	}

	option
}

fn finalize_handler(req: &mut Request) -> IronResult<Response> {
	let final_filename = match req.get_ref::<UrlEncodedBody>() {
		Ok(ref hashmap) => {
			let files = &hashmap["output"];
			if files.len() > 0 {
				let current_directory = current_dir().unwrap();
				let file = format!("{}/{}", current_directory.to_str().unwrap(), &files[0]);
				let final_filename = available_filename("images/final_output", ".jpg");
				if let Err(_) = symlink(file, &final_filename) {
					return Ok(Response::with(status::InternalServerError));
				}

				final_filename
			} else {
				return Ok(Response::with(status::BadRequest));
			}
		},
		Err(ref e) => {
			println!("{:?}", e);
			return Ok(Response::with(status::InternalServerError));
		},
	};

	// Set Last Final Output
	let mutex = req.get::<Write<LastFinalOutput>>().unwrap();
	let mut last_final_output = mutex.lock().unwrap();
	*last_final_output = final_filename.clone();

	println!("Final output: {}", &final_filename);

	Ok(Response::with(status::Ok))
}

fn main() {
    if let Err(_) = fs::create_dir_all("images") {
        panic!("Couldn't create the folder `images`.");
    }

    if !file_exists("public/tv") {
        panic!("The folder `public/tv` doesn't exist.");
    }

    let current_directory = current_dir().unwrap();
    let _ = symlink(format!("{}/images", current_directory.to_str().unwrap()), "public/tv/images");

    let mut router = Router::new();
    router.get("/result", result_handler);
    router.post("/capture", capture_handler);
    router.post("/post_process", post_process_handler);
    router.post("/finalize", finalize_handler);

    let mut mount = Mount::new();
    mount.mount("/", Static::new(Path::new("public")));
    mount.mount("/tv", Static::new(Path::new("public/tv")));
    mount.mount("/api", router);

    let mut chain = Chain::new(mount);
    chain.link_before(Write::<LastRawOutput>::one(String::from("")));
    chain.link_before(Write::<LastOutput>::one(String::from("")));
    chain.link_before(Write::<LastFinalOutput>::one(find_last_output("images/final_output", ".jpg")));

    println!("Serving at {}", ADDRESS);
    Iron::new(chain).http(ADDRESS).unwrap();
}
