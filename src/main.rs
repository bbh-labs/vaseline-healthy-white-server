extern crate iron;
extern crate mount;
extern crate router;
extern crate staticfile;

// Standard Library
use std::fs;
use std::fs::metadata;
use std::path::Path;
use std::process::Command;
use std::os::unix::fs::symlink;

// Iron
use iron::prelude::*;
use iron::status;
use mount::Mount;
use router::Router;
use staticfile::Static;

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
			return filename.to_string()
		}
	}

	"".to_string()
}

fn capture_handler(_req: &mut Request) -> IronResult<Response> {
	let output_filename = available_filename("images/raw_output", ".jpg");
	let output = Command::new("gphoto2")
	                     .arg("--auto-detect")
						 .arg("--capture-image-and-download")
						 .arg(output_filename)
						 .output()
						 .unwrap_or_else(|e| { panic!("failed to execute process: {}", e) });

	let response: Response;

	let output_message = 
		if output.status.success() {
			response = Response::with(status::Ok);
			output.stdout
		} else {
			response = Response::with(status::InternalServerError);
			output.stderr
		};

	println!("{}", String::from_utf8(output_message).unwrap());

	Ok(response)
}

fn post_process_handler(_req: &mut Request) -> IronResult<Response> {
	let style_filename  = "style.xmp";
	let input_filename = available_filename("images/raw_output", ".jpg");
	let output_filename = available_filename("images/output", ".jpg");
	let output = Command::new("darktable-cli")
	                     .arg(&input_filename)
						 .arg(&style_filename)
						 .arg(&output_filename)
						 .output()
						 .unwrap_or_else(|e| { panic!("failed to execute process: {}", e) });

	let response: Response;

	let output_message = 
		if output.status.success() {
			response = Response::with((status::Ok, output_filename));
			output.stdout
		} else {
			response = Response::with(status::InternalServerError);
			output.stderr
		};

	println!("{}", String::from_utf8(output_message).unwrap());

	Ok(response)
}

fn main() {
	if let Err(_) = fs::create_dir_all("images") {
		panic!("Couldn't create the folder `images`.");
	}

	if !file_exists("public/tv") {
		panic!("The folder `public/tv` doesn't exist.");
	}

	let _ = symlink("images", "public/tv/images");

	let mut router = Router::new();
	router.post("/capture", capture_handler);
	router.post("/post_process", post_process_handler);

	let mut mount = Mount::new();
	mount.mount("/", Static::new(Path::new("public")));
	mount.mount("/tv", Static::new(Path::new("public/tv")));
	mount.mount("/api", router);

	Iron::new(mount).http("localhost:8080").unwrap();
}
