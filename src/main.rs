extern crate iron;
extern crate mount;
extern crate persistent;
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
use iron::typemap::Key;
use mount::Mount;
use persistent::Write;
use router::Router;
use staticfile::Static;

#[derive(Copy, Clone)]
pub struct LastResult;
impl Key for LastResult { type Value = String; }

#[derive(PartialEq)]
pub enum Stage {
	Idle,
	Capturing,
	PostProcessing
}

#[derive(Copy, Clone)]
pub struct CurrentStage;
impl Key for CurrentStage { type Value = Stage; }

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

fn result_handler(req: &mut Request) -> IronResult<Response> {
	let mutex = req.get::<Write<LastResult>>().unwrap();
	let last_result = mutex.lock().unwrap();
	Ok(Response::with((status::Ok, (*last_result).clone())))
}

fn capture_handler(req: &mut Request) -> IronResult<Response> {
	let mutex = req.get::<Write<CurrentStage>>().unwrap();
	let mut stage = mutex.lock().unwrap();
	if *stage != Stage::Idle {
		return Ok(Response::with(status::BadRequest));
	}
	*stage = Stage::Capturing;

	let output_filename = available_filename("images/raw_output", ".jpg");
	let output = Command::new("gphoto2")
	                     .arg("--auto-detect")
						 .arg("--capture-image-and-download")
						 .arg("--filename")
						 .arg(&output_filename)
						 .output()
						 .unwrap_or_else(|e| { panic!("failed to execute process: {}", e) });
	println!("Raw output filename: {}", &output_filename);

	let response: Response;

	let output_message = 
		if output.status.success() {
			let mutex = req.get::<Write<LastResult>>().unwrap();
			let mut last_result = mutex.lock().unwrap();
			*last_result = output_filename;
			response = Response::with(status::Ok);
			output.stdout
		} else {
			response = Response::with(status::InternalServerError);
			output.stderr
		};

	println!("{}", String::from_utf8(output_message).unwrap());
	
	*stage = Stage::Idle;

	Ok(response)
}

fn post_process_handler(req: &mut Request) -> IronResult<Response> {
	let mutex = req.get::<Write<CurrentStage>>().unwrap();
	let mut stage = mutex.lock().unwrap();
	if *stage != Stage::Idle {
		return Ok(Response::with(status::BadRequest));
	}
	*stage = Stage::PostProcessing;

	let mutex = req.get::<Write<LastResult>>().unwrap();
	let last_result = mutex.lock().unwrap();

	let style_filename  = "style.xmp";
	let input_filename = last_result;
	let output_filename = available_filename("images/output", ".jpg");
	let output = Command::new("darktable-cli")
	                     .arg((*input_filename).clone())
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

	*stage = Stage::Idle;

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
	router.get("/result", result_handler);
	router.post("/capture", capture_handler);
	router.post("/post_process", post_process_handler);

	let mut mount = Mount::new();
	mount.mount("/", Static::new(Path::new("public")));
	mount.mount("/tv", Static::new(Path::new("public/tv")));
	mount.mount("/api", router);

	let mut chain = Chain::new(mount);
	chain.link_before(Write::<LastResult>::one("".to_string()));
	chain.link_before(Write::<CurrentStage>::one(Stage::Idle));

	Iron::new(chain).http("localhost:8080").unwrap();
}
