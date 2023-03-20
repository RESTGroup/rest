use std::collections::HashMap;
use std::convert::TryInto;
use rayon::prelude::*;

use super::becke_partitioning;
use super::bragg;
use super::bse;
use super::lebedev;
use super::prune::{nwchem_prune, sg1_prune};
use super::radial;
use super::parameters::LEBEDEV_NGRID;

pub fn atom_grid_bse(
    basis_set: &str,
    radial_precision: f64,
    min_num_angular_points: usize,
    max_num_angular_points: usize,
    proton_charges: Vec<i32>,
    center_index: usize,
    center_coordinates_bohr: Vec<(f64, f64, f64)>,
    hardness: usize,
    pruning: String,
    rad_grid_method: String,
    level: usize,
) -> (Vec<(f64, f64, f64)>, Vec<f64>) {
    let (alpha_min, alpha_max) =
        bse::ang_min_and_max(basis_set, proton_charges[center_index] as usize);

    atom_grid(
        alpha_min,
        alpha_max,
        radial_precision,
        min_num_angular_points,
        max_num_angular_points,
        proton_charges,
        center_index,
        center_coordinates_bohr,
        hardness,
        pruning,
        rad_grid_method,
        level,
    )
}

pub fn atom_grid(
    alpha_min: HashMap<usize, f64>,
    alpha_max: f64,
    radial_precision: f64,
    min_num_angular_points: usize,
    max_num_angular_points: usize,
    proton_charges: Vec<i32>,
    center_index: usize,
    center_coordinates_bohr: Vec<(f64, f64, f64)>,
    hardness: usize,
    pruning: String,
    rad_grid_method: String,
    level: usize,
) -> (Vec<(f64, f64, f64)>, Vec<f64>) {


/*
    //Generate radial grid through lmg method
    let (rs, weights_radial) = radial::radial_grid_lmg(
        alpha_min,
        alpha_max,
        radial_precision,
        proton_charges[center_index],
    );
*/
    //Generate radial grid through treutler method

    let (rs, weights_radial) = 
        if rad_grid_method == String::from("treutler") {
            radial::radial_grid_treutler(default_radial_num(proton_charges[center_index] as usize, level))
        }
        else if rad_grid_method == String::from("gc2nd") {
            radial::radial_grid_gc2nd(default_radial_num(proton_charges[center_index] as usize, level))
        }
        else {
            radial::radial_grid_lmg(
                alpha_min,
                alpha_max,
                radial_precision,
                proton_charges[center_index],
            )
        };
        
    //println!("radial num = {}", default_radial_num(proton_charges[center_index] as usize));
    // factors match DIRAC code
    //println!("rs = {:?}, w = {:?}", rs, weights_radial);
    let rb = bragg::get_bragg_angstrom(proton_charges[center_index]) / (5.0 * 0.529177249);

    let mut coordinates = Vec::new();
    let mut weights = Vec::new();

    let pi = std::f64::consts::PI;

    let cx = center_coordinates_bohr[center_index].0;
    let cy = center_coordinates_bohr[center_index].1;
    let cz = center_coordinates_bohr[center_index].2;

    //get prune parameter
    let prune_method: usize = {    
        if pruning == String::from("sg1"){
            1
        }
        else if pruning == String::from("nwchem"){
            2
        }
        else {
            0
        }
    };

    //generate prune list
    //rs: radial grid coordinate 
    //let mut ang_array = vec![0usize; rs.len()];

    let ang_array = 
    if prune_method == 1 {
        sg1_prune(proton_charges[center_index].try_into().unwrap(), &rs, rs.len())
    }
    else if prune_method == 2 {
        let n_ang = default_angular_num(proton_charges[center_index].try_into().unwrap(), level);
        nwchem_prune(proton_charges[center_index].try_into().unwrap(), &rs, n_ang, rs.len())
    }
    else {
        vec![0usize; rs.len()]
    };

    //println!("ang array is {:?}", ang_array);

    let mut radial_coord_index = 0; // index of coord in radial coordinate array
    
    for (&r, &weight_radial) in rs.iter().zip(weights_radial.iter()) {
        // we read the angular grid at each radial step because of pruning
        // this can be optimized
        let mut num_angular = max_num_angular_points;

        //probable meaning:
        //r: radial grid coordinate
        //rb: Bohr/Bragg(?) related radius

        let (coordinates_angular, weights_angular) = 
        if prune_method == 0 {
            if r < rb {
                num_angular = ((max_num_angular_points as f64) * r / rb) as usize;
                num_angular = lebedev::get_closest_num_angular(num_angular);
                if num_angular < min_num_angular_points {
                    num_angular = min_num_angular_points;
                }
            }
            lebedev::angular_grid(num_angular)
        }
        else {
                lebedev::angular_grid(ang_array[radial_coord_index])
            };

        radial_coord_index += 1;


        //change polar coord to cartesian coord
        //let wt = 4.0 * pi * weight_radial; //different from pyscf
        let wt = 4.0 * pi * r.powf(2.0) * weight_radial; //this is pyscf expr
        for (&xyz, &weight_angular) in coordinates_angular.iter().zip(weights_angular.iter()) {
            let x = cx + r * xyz.0;
            let y = cy + r * xyz.1;
            let z = cz + r * xyz.2;

            coordinates.push((x, y, z));
            weights.push(wt * weight_angular);
        }
    }

    if center_coordinates_bohr.len() > 1 {
        let w_partitioning: Vec<f64> = coordinates
            .par_iter()
            .map(|c| {
                becke_partitioning::partitioning_weight(
                    center_index,
                    &center_coordinates_bohr,
                    &proton_charges,
                    *c,
                    hardness,
                )
            })
            .collect();

        for (i, w) in weights.iter_mut().enumerate() {
            *w *= w_partitioning[i];
        }
    }

    (coordinates, weights)
}

pub fn default_radial_num(nuc: usize, level: usize) -> usize {
    //level = 3
    let tab = [2 , 10, 18, 36, 54, 86, 118];
    //let num = [50, 75, 80, 90, 95, 100, 105];
    //                    Period    1   2   3   4   5   6   7       // level
    let num =  [[ 10, 15, 20, 30, 35, 40, 50],     // 0
                                [ 30, 40, 50, 60, 65, 70, 75],     // 1
                                [ 40, 60, 65, 75, 80, 85, 90],     // 2
                                [ 50, 75, 80, 90, 95,100,105],     // 3
                                [ 60, 90, 95,105,110,115,120],     // 4
                                [ 70,105,110,120,125,130,135],     // 5
                                [ 80,120,125,135,140,145,150],     // 6
                                [ 90,135,140,150,155,160,165],     // 7
                                [100,150,155,165,170,175,180],     // 8
                                [200,200,200,200,200,200,200]];    // 9

    let mut j = 0;
    for i in tab {
        if nuc > i {
            j+=1;
        }
    }
    //println!("j = {}", j);
    return num[level][j];
}

pub fn default_angular_num(nuc: usize, level: usize) -> usize {
    //level = 3
    let tab = [2 , 10, 18, 36, 54, 86, 118];
    //let num = [302, 302, 434, 434, 434, 434, 434];
    //                 Period    1   2    3    4    5    6    7                    // level
    let num =  [[50, 86, 110, 110, 110, 110, 110 ],                // 0
                                [110, 194, 194, 194, 194, 194, 194 ],              // 1
                                [194, 302, 302, 302, 302, 302, 302 ],              // 2
                                [302, 302, 434, 434, 434, 434, 434 ],              // 3
                                [434, 590, 590, 590, 590, 590, 590 ],              // 4
                                [590, 770, 770, 770, 770, 770, 770 ],              // 5
                                [770, 974, 974, 974, 974, 974, 974 ],              // 6
                                [974, 1202, 1202, 1202, 1202, 1202, 1202 ],        // 7
                                [1202, 1202, 1202, 1202, 1202, 1202, 1202 ],       // 8
                                [1454, 1454, 1454, 1454, 1454, 1454, 1454 ]];      // 9
    let mut j = 0;
    for i in tab {
        if nuc > i {
            j+=1;
        }
    }
    //println!("j = {}", j);
    return num[level][j];
}


fn get_closest_n_ang(n: usize) -> usize {
    let leb = &LEBEDEV_NGRID;
    if n == 1 {
        return 1;
    }
    for i in 0..leb.len() {
        if n <= leb[i] {
            if (leb[i] - n) <= (n - leb[i-1]) {
                return leb[i];
            }
            else {
                return leb[i-1];
            }
        }
    }
    panic!("input n_ang too high");
}