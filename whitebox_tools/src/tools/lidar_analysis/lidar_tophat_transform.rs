/* 
This tool is part of the WhiteboxTools geospatial analysis library.
Authors: Dr. John Lindsay
Created: June 22, 2017
Last Modified: January 21, 2018
License: MIT
*/
extern crate time;
extern crate num_cpus;

use std::env;
use std::f64;
use std::path;
use std::io::{Error, ErrorKind};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use lidar::*;
use tools::*;
use structures::FixedRadiusSearch2D;

pub struct LidarTophatTransform {
    name: String,
    description: String,
    toolbox: String,
    parameters: Vec<ToolParameter>,
    example_usage: String,
}

impl LidarTophatTransform {
    pub fn new() -> LidarTophatTransform { // public constructor
        let name = "LidarTophatTransform".to_string();
        let toolbox = "LiDAR Tools".to_string();
        let description = "Performs a white top-hat transform on a Lidar dataset; as an estimate of height above ground, this is useful for modelling the vegetation canopy".to_string();
        
        let mut parameters = vec![];
        parameters.push(ToolParameter{
            name: "Input File".to_owned(), 
            flags: vec!["-i".to_owned(), "--input".to_owned()], 
            description: "Input LiDAR file.".to_owned(),
            parameter_type: ParameterType::ExistingFile(ParameterFileType::Lidar),
            default_value: None,
            optional: false
        });

        parameters.push(ToolParameter{
            name: "Output File".to_owned(), 
            flags: vec!["-o".to_owned(), "--output".to_owned()], 
            description: "Output LiDAR file.".to_owned(),
            parameter_type: ParameterType::NewFile(ParameterFileType::Lidar),
            default_value: None,
            optional: false
        });

        parameters.push(ToolParameter{
            name: "Search Radius".to_owned(), 
            flags: vec!["--radius".to_owned()], 
            description: "Search Radius.".to_owned(),
            parameter_type: ParameterType::Float,
            default_value: Some("1.0".to_owned()),
            optional: false
        });
  
        let sep: String = path::MAIN_SEPARATOR.to_string();
        let p = format!("{}", env::current_dir().unwrap().display());
        let e = format!("{}", env::current_exe().unwrap().display());
        let mut short_exe = e.replace(&p, "").replace(".exe", "").replace(".", "").replace(&sep, "");
        if e.contains(".exe") {
            short_exe += ".exe";
        }
        let usage = format!(">>.*{0} -r={1} -v --wd=\"*path*to*data*\" -i=\"input.las\" -o=\"output.las\" --radius=10.0", short_exe, name).replace("*", &sep);
    
        LidarTophatTransform { 
            name: name, 
            description: description, 
            toolbox: toolbox,
            parameters: parameters, 
            example_usage: usage 
        }
    }
}

impl WhiteboxTool for LidarTophatTransform {
    fn get_source_file(&self) -> String {
        String::from(file!())
    }
    
    fn get_tool_name(&self) -> String {
        self.name.clone()
    }

    fn get_tool_description(&self) -> String {
        self.description.clone()
    }

    fn get_tool_parameters(&self) -> String {
        let mut s = String::from("{\"parameters\": [");
        for i in 0..self.parameters.len() {
            if i < self.parameters.len() - 1 {
                s.push_str(&(self.parameters[i].to_string()));
                s.push_str(",");
            } else {
                s.push_str(&(self.parameters[i].to_string()));
            }
        }
        s.push_str("]}");
        s
    }

    fn get_example_usage(&self) -> String {
        self.example_usage.clone()
    }

    fn get_toolbox(&self) -> String {
        self.toolbox.clone()
    }

    fn run<'a>(&self, args: Vec<String>, working_directory: &'a str, verbose: bool) -> Result<(), Error> {
        let mut input_file: String = "".to_string();
        let mut output_file: String = "".to_string();
        let mut search_radius: f64 = -1.0;
        
        // read the arguments
        if args.len() == 0 {
            return Err(Error::new(ErrorKind::InvalidInput, "Tool run with no paramters."));
        }
        for i in 0..args.len() {
            let mut arg = args[i].replace("\"", "");
            arg = arg.replace("\'", "");
            let cmd = arg.split("="); // in case an equals sign was used
            let vec = cmd.collect::<Vec<&str>>();
            let mut keyval = false;
            if vec.len() > 1 { keyval = true; }
            if vec[0].to_lowercase() == "-i" || vec[0].to_lowercase() == "--input" {
                if keyval {
                    input_file = vec[1].to_string();
                } else {
                    input_file = args[i+1].to_string();
                }
            } else if vec[0].to_lowercase() == "-o" || vec[0].to_lowercase() == "--output" {
                if keyval {
                    output_file = vec[1].to_string();
                } else {
                    output_file = args[i+1].to_string();
                }
            } else if vec[0].to_lowercase() == "-radius" || vec[0].to_lowercase() == "--radius" {
                if keyval {
                    search_radius = vec[1].to_string().parse::<f64>().unwrap();
                } else {
                    search_radius = args[i+1].to_string().parse::<f64>().unwrap();
                }
            }
        }

        if verbose {
            println!("***************{}", "*".repeat(self.get_tool_name().len()));
            println!("* Welcome to {} *", self.get_tool_name());
            println!("***************{}", "*".repeat(self.get_tool_name().len()));
        }

        let sep = path::MAIN_SEPARATOR;
        if !input_file.contains(sep) {
            input_file = format!("{}{}", working_directory, input_file);
        }
        if !output_file.contains(sep) {
            output_file = format!("{}{}", working_directory, output_file);
        }

        if verbose { println!("Reading input LAS file..."); }
        let input = match LasFile::new(&input_file, "r") {
            Ok(lf) => lf,
            Err(err) => panic!("Error reading file {}: {}", input_file, err),
        };

        let start = time::now();

        if verbose { println!("Performing analysis..."); }

        let n_points = input.header.number_of_points as usize;
        let num_points: f64 = (input.header.number_of_points - 1) as f64; // used for progress calculation only

        let mut progress: i32;
        let mut old_progress: i32 = -1;
        let mut frs: FixedRadiusSearch2D<usize> = FixedRadiusSearch2D::new(search_radius);
        for i in 0..n_points {
            let p: PointData = input.get_point_info(i);
            frs.insert(p.x, p.y, i);
            if verbose {
                progress = (100.0_f64 * i as f64 / num_points) as i32;
                if progress != old_progress {
                    println!("Binning points: {}%", progress);
                    old_progress = progress;
                }
            }
        }

        let mut neighbourhood_min = vec![f64::MAX; n_points];
        let mut residuals = vec![f64::MIN; n_points];
        
        /////////////
        // Erosion //
        /////////////

        let frs = Arc::new(frs); // wrap FRS in an Arc
        let input = Arc::new(input); // wrap input in an Arc
        let num_procs = num_cpus::get();
        let (tx, rx) = mpsc::channel();
        for tid in 0..num_procs {
            let frs = frs.clone();
            let input = input.clone();
            let tx = tx.clone();
            thread::spawn(move || {
                let mut index_n: usize;
                let mut z_n: f64;
                let mut min_z: f64;
                for i in (0..n_points).filter(|point_num| point_num % num_procs == tid) {
                    let p: PointData = input.get_point_info(i);
                    let ret = frs.search(p.x, p.y);
                    min_z = f64::MAX;
                    for j in 0..ret.len() {
                        index_n = ret[j].0;
                        z_n = input.get_point_info(index_n).z;
                        if z_n < min_z {
                            min_z = z_n;
                        }
                    }
                    tx.send((i, min_z)).unwrap();
                }
            });
        }

        for i in 0..n_points {
            let data = rx.recv().unwrap();
            neighbourhood_min[data.0] = data.1;
            if verbose {
                progress = (100.0_f64 * i as f64 / num_points) as i32;
                if progress != old_progress {
                    println!("Erosion: {}%", progress);
                    old_progress = progress;
                }
            }
        }

        //////////////
        // Dilation //
        //////////////
        let neighbourhood_min = Arc::new(neighbourhood_min); // wrap neighbourhood_min in an Arc
        for tid in 0..num_procs {
            let frs = frs.clone();
            let input = input.clone();
            let neighbourhood_min = neighbourhood_min.clone();
            let tx = tx.clone();
            thread::spawn(move || {
                let mut index_n: usize;
                let mut z_n: f64;
                let mut max_z: f64;
                for i in (0..n_points).filter(|point_num| point_num % num_procs == tid) {
                    let p: PointData = input.get_point_info(i);
                    let ret = frs.search(p.x, p.y);
                    max_z = f64::MIN;
                    for j in 0..ret.len() {
                        index_n = ret[j].0;
                        z_n = neighbourhood_min[index_n];
                        // z_n = input.get_point_info(index_n).z;
                        if z_n > max_z {
                            max_z = z_n;
                        }
                    }
                    tx.send((i, max_z)).unwrap();
                }
            });
        }

        for i in 0..n_points {
            let data = rx.recv().unwrap();
            let z = input.get_point_info(data.0).z;
            residuals[data.0] = z - data.1;
            if verbose {
                progress = (100.0_f64 * i as f64 / num_points) as i32;
                if progress != old_progress {
                    println!("Dilation: {}%", progress);
                    old_progress = progress;
                }
            }
        }

        // now output the data
        let mut output = LasFile::initialize_using_file(&output_file, &input);
        output.header.system_id = "EXTRACTION".to_string();

        for i in 0..n_points {
            let pr = input.get_record(i);
            let pr2: LidarPointRecord;
            match pr {
                LidarPointRecord::PointRecord0 { mut point_data }  => {
                    point_data.z = residuals[i];
                    pr2 = LidarPointRecord::PointRecord0 { point_data: point_data };

                },
                LidarPointRecord::PointRecord1 { mut point_data, gps_data } => {
                    point_data.z = residuals[i];
                    pr2 = LidarPointRecord::PointRecord1 { point_data: point_data, gps_data: gps_data };
                },
                LidarPointRecord::PointRecord2 { mut point_data, rgb_data } => {
                    point_data.z = residuals[i];
                    pr2 = LidarPointRecord::PointRecord2 { point_data: point_data, rgb_data: rgb_data };
                },
                LidarPointRecord::PointRecord3 { mut point_data, gps_data, rgb_data } => {
                    point_data.z = residuals[i];
                    pr2 = LidarPointRecord::PointRecord3 { point_data: point_data,
                        gps_data: gps_data, rgb_data: rgb_data};
                },
            }
            output.add_point_record(pr2);
            if verbose {
                progress = (100.0_f64 * i as f64 / num_points) as i32;
                if progress != old_progress {
                    println!("Saving data: {}%", progress);
                    old_progress = progress;
                }
            }
        }

        let end = time::now();
        let elapsed_time = end - start;

        if verbose { println!("Writing output LAS file..."); }
        let _ = match output.write() {
            Ok(_) => println!("Complete!"),
            Err(e) => println!("error while writing: {:?}", e),
        };

        println!("{}", &format!("Elapsed Time (excluding I/O): {}", elapsed_time).replace("PT", ""));

        Ok(())
    }
}

