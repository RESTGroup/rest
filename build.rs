extern crate dunce;
use std::{env, process::Command, fs};
//use std::path::PathBuf;

fn main() -> miette::Result<()> {

    let external_dir = if let Ok(external_dir) = env::var("REST_EXT_DIR") {
        external_dir
    } else {"".to_string()};

    build_dftd3and4();

    generate_libxc_names_and_values();

    let library_names = ["restmatr","openblas","xc","hdf5","rest2fch"];
    library_names.iter().for_each(|name| {
        println!("cargo:rustc-link-lib={}",*name);
    });
    let library_path = [
        dunce::canonicalize(&external_dir).unwrap(),
    ];
    library_path.iter().for_each(|path| {
        println!("cargo:rustc-link-search={}",env::join_paths(&[path]).unwrap().to_str().unwrap())
    });

    Ok(())

}

fn build_dftd3and4() {
    let rest_dir = if let Ok(rest_dir) = env::var("REST_HOME") {
        rest_dir
    } else {"".to_string()};
    let external_dir = if let Ok(external_dir) = env::var("REST_EXT_DIR") {
        external_dir
    } else {"".to_string()};
    let external_inc = if let Ok(external_inc) = env::var("REST_EXT_INC") {
        external_inc
    } else {"".to_string()};

    let fortran_compiler = if let Ok(fortran_compiler) = env::var("REST_FORTRAN_COMPILER") {
        fortran_compiler
    } else {"gfortran".to_string()};

    // compile dftd3_rest
    let dftd3_rest_file = format!("{}/rest/src/external_libs/dftd3_rest.f90", &rest_dir);
    let dftd3_rest_libr = format!("{}/libdftd3_rest.so",&external_dir);
    let dftd3_rest_link = format!("-L{}",&external_dir);
    let dftd3_rest_include = format!("-I{}/{}",&external_inc,"dftd3");
    
    Command::new(&fortran_compiler).arg("-shared").arg("-fPIC").arg("-O2")
        .arg(&dftd3_rest_file)
        .arg("-o").arg(&dftd3_rest_libr)
        .arg(&dftd3_rest_link).arg("-ls-dftd3")
        .arg(&dftd3_rest_include).status().unwrap();


    // compile dftd4_rest
    let dftd4_rest_file = format!("{}/rest/src/external_libs/dftd4_rest.f90", &rest_dir.to_string());
    let dftd4_rest_libr = format!("{}/libdftd4_rest.so",&external_dir.to_string());
    let dftd4_rest_link = format!("-L{}",&external_dir.to_string());
    let dftd4_rest_include = format!("-I{}/{}",&external_inc.to_string(),"dftd4");

    Command::new(&fortran_compiler).arg("-shared").arg("-fPIC").arg("-O2")
        .arg(&dftd4_rest_file)
        .arg("-o").arg(&dftd4_rest_libr)
        .arg(&dftd4_rest_link).arg("-ldftd4")
        .arg(&dftd4_rest_include).status().unwrap();

    println!("cargo:rerun-if-changed={}", &dftd3_rest_libr);
    println!("cargo:rerun-if-changed={}", &dftd4_rest_libr);
    println!("cargo:rerun-if-changed={}", &dftd3_rest_file);
    println!("cargo:rerun-if-changed={}", &dftd4_rest_file);

    let library_names = ["s-dftd3","dftd4","dftd3_rest","dftd4_rest"];
    library_names.iter().for_each(|name| {
        println!("cargo:rustc-link-lib={}",*name);
    });

}

fn generate_libxc_names_and_values() {
    // Retrieve the REST_HOME environment variable or use an empty string if not set
    let rest_dir = if let Ok(rest_dir) = env::var("REST_HOME") {
        rest_dir
    } else {"".to_string()};

    // Define the output directory and destination path for the generated libxc code
    let out_dir = format!("{}/rest/src/dft/libxc", &rest_dir);
    let dest_path = format!("{}/rest/src/dft/libxc/names_and_values.rs", &rest_dir);
    let libxc_full_path = format!("{}/rest/src/dft/libxc/libxc_full.rs", &rest_dir);

    // if std::path::Path::new(&dest_path).exists() {
    //     println!("File {} already exists. Skipping libxc code generation.", dest_path);
    //     return;
    // }
    println!("Generating libxc code from {} to {}", libxc_full_path, dest_path);

    // Read the contents of libxc_full.rs and parse lines that start with "pub const"
    let constants_code = fs::read_to_string(libxc_full_path).unwrap();
    let const_lines: Vec<_> = constants_code
        .lines()
        .filter(|line| line.trim().starts_with("pub const"))
        .map(|line| {
            let parts: Vec<_> = line.split_whitespace().collect();
            let name = parts[2].trim_end_matches(':');
            (name, line)
        })
        .collect();

    // Generate the MAP code by filtering and formatting the parsed lines
    let generated_code = format!(
        "// This map of libxc names and values is auto-generated by reading libxc_full.rs\npub const MAP: &[(&str, usize)] = &[\n{}\n];",
        const_lines
            .iter()
            .filter(|(_, line)| line.contains("u32"))
            .map(|(name, line)| {
                let value: Vec<_> = line.split('=').collect();
                let value = value[1].trim().trim_end_matches(';');
                format!("    (\"{}\", {}),", name, value)
            })
            .collect::<Vec<_>>()
            .join("\n")
    );

    // Write the generated code to the destination path
    fs::write(&dest_path, generated_code).unwrap();
    println!("cargo:rerun-if-changed={}", dest_path);
}
