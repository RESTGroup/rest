use std::ops::Range;
use crate::scf_io::SCF;
use crate::{geom_io,dft,molecule_io};
use crate::dft::Grids as dftgrids;
use crate::molecule_io::Molecule;
use rest_tensors::{MatrixFull, RIFull, ERIFull};
use tensors::{TensorSlice, TensorSliceMut};
use std::cmp::Ordering;
use rand::distributions::normal::StandardNormal;

mod lib;

/* tested, right version.
Given grid points and center of each clusters, classify points to nearest cluster centers.
Input:
Rgrid, c_mu(original point)
output: 
    ind_R, dist_R */
pub fn cvt_classification(grids: &dft::Grids, c_mu: &Vec<[f64;3]>) -> (Vec<usize>,Vec<f64>){
    //consider par, tested
    let rgrid = &grids.coordinates;
    let num_grids = grids.weights.len();  
    let mut ind_r:Vec<usize> = vec![0; num_grids];
    let mut dist_r:Vec<f64> = vec![0.0;num_grids];
    let num_mu = c_mu.len();
    //let mut r_min = 0.0; 
    for i in 0..num_grids{
        let mut r_c_mu:Vec<f64> = vec![0.0; num_mu];
        for j in 0..c_mu.len(){
            r_c_mu[j] = rgrid[i].iter().zip(c_mu[j].iter()).fold(0.0,|r,(ac,gc)| {r + (ac-gc).powf(2.0)}).sqrt();
        }
        ind_r[i] = index_of_min(&mut r_c_mu);
        dist_r[i] = r_c_mu[ind_r[i]];
    }//further zip
    (ind_r, dist_r)
}

/*  tested
    Given classification for grid points, regenerate cluster centers by taking mean value.
    Input: Rgrid, ind_R(from CVT_classification), n_mu, lambda_R, c_mu_old
    Output: c_mu_new(after update)
    Update formula:
        If no points corresponds to it: stay there.
        If some points corresponds to it: update normally */
pub fn cvt_update_cmu(grids: &dft::Grids, c_mu_old: &Vec<[f64;3]>, ind_r: &Vec<usize>) -> Vec<[f64;3]>{
    let num_mu = c_mu_old.len();
    let rgrid = &grids.coordinates;
    let lambda_r = &grids.weights;
    let num_grids = grids.weights.len(); 
    let mut c_mu_save = c_mu_old.clone();

    //Added together
    let mut c_mu_tmp:Vec<[f64; 3]> = vec![[0.0;3];num_mu];
    let mut weight_sum: Vec<f64> = vec![0.0; num_mu];
    for i in 0..num_grids{
        //c_mu_tmp[ind_r[i]] += lambda_r[ind_r[i]] * rgrid[ind_r[i]];
        c_mu_tmp[ind_r[i]].iter_mut().zip(rgrid[i].iter()).for_each(|(x,y)|{
            *x += lambda_r[i] * y;
        });
        weight_sum[ind_r[i]] += lambda_r[i];
    }

    /* for i in 0..num_grids{
        //c_mu_tmp[ind_r[i]] += lambda_r[ind_r[i]] * rgrid[ind_r[i]];
        c_mu_tmp[ind_r[i]].iter_mut().zip(rgrid[i].iter()).for_each(|(x,y)|{
            *x += lambda_r[ind_r[i]] * y;
        });
        weight_sum[ind_r[i]] += lambda_r[ind_r[i]];
    } */
    //further zip

    //Renew c_mu
    let non_zero_ind = weight_sum.iter()
    .enumerate()
    .filter(|(_, &r)| r >= 1e-8)
    .map(|(index, _)| index)
    .collect::<Vec<_>>();//1e-8 could be adjusted.
    for i in 0..non_zero_ind.len(){
        c_mu_save[non_zero_ind[i]].iter_mut().zip(c_mu_tmp[non_zero_ind[i]].iter()).for_each(|(x,y)|{
            *x = y / weight_sum[non_zero_ind[i]];
        })
    }

    c_mu_save
}

/* tested.
find points in Rgrid minimizing 2-norm to c_mu[i]
ind_sorted: 归属于哪一个cluster */
pub fn cvt_find_corresponding_point(grids: &dft::Grids, c_mu: &Vec<[f64;3]>, ind_r: &Vec<usize>) -> Vec<usize> {
    let num_mu = c_mu.len();
    let rgrid = &grids.coordinates;
    let num_grids = grids.weights.len(); 
    let mut ind_mu: Vec<usize> = vec![0; num_mu];;

    //find nearest center
    let mut min_dist = vec![1.0e10; num_mu];
    //let mut r_grid_core:Vec<f64> = vec![0.0; num_grids];
  
    //find those whose clusters have been changed
    for i in 0..num_grids{
        let mut value = rgrid[i].iter().zip(c_mu[ind_r[i]].iter()).fold(0.0,|r,(ac,gc)| {r + (ac-gc).powf(2.0)}).sqrt();
        if value < min_dist[ind_r[i]]{
            ind_mu[ind_r[i]] = i ;
            min_dist[ind_r[i]] = value;
        }
    }
    let need_search_full_rgrid = ind_mu.iter().enumerate().filter(|(_, &r)| r == 0).map(|(index,_)|index).collect::<Vec<_>>();
    for i in need_search_full_rgrid{
        let mut dist_to_all_grids: Vec<f64> = vec![0.0; num_grids];
        dist_to_all_grids.iter_mut().zip(rgrid).for_each(|(x,y)|{
            *x = y.iter().zip(c_mu[i].iter()).fold(0.0,|r,(ac,gc)| {r + (ac-gc).powf(2.0)}).sqrt();
        });
        ind_mu[i] = index_of_min(&mut dist_to_all_grids);   
    }


    ind_mu
}

pub fn cvt_isdf(grids:& dft::Grids, n_mu: usize) ->  (Vec<usize>, f64){
    let lambda_r = &grids.weights;
    let rgrid = &grids.coordinates;
    let effective_ind = lambda_r.iter()
        .enumerate()
        .filter(|(_, &r)| r >= 1e-8)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if n_mu >= effective_ind.len() {
        panic!("n_mu is smaller than effective_ind!");
    }
    
    /* let mut new_rgrid: Vec<[f64; 3]> = vec![[0.0;3];effective_ind.len()];
    let mut new_weight: Vec<f64> = vec![0.0;effective_ind.len()];
    new_rgrid.iter_mut().zip(effective_ind.iter().zip(new_weight.iter_mut())).for_each(|(x,(y,z))|{
        *x = rgrid[*y];
        *z = lambda_r[*y];
    }); */

    let mut rng = rand::thread_rng();
    let mut c_mu: Vec<[f64;3]> = vec![[0.0;3];n_mu];
    c_mu.iter_mut().for_each(|[x,y,z]|{
        let StandardNormal(a) = rand::random();
        *x =a;
        
        let StandardNormal(b) = rand::random();
        *y =b;
        let StandardNormal(c) = rand::random();
        *z =c;
    });
    //println!("{:?}",&c_mu);
    println!("num_grids:{}; n_mu:{}", &grids.weights.len(), n_mu);
    /* for i in (0..100){
        println!("example_numgrids:{:?}", &grids.coordinates[i]);
    } */
    

    let max_iter = 300;
    let mut class_result = (vec![0usize; n_mu],vec![0.0; n_mu]);
    let mut ind_r = vec![0usize; n_mu];
    let mut dist_r = vec![0.0; n_mu];
    let mut count = 0;
    let mut dist = 0.0;
    let mut c_mu_new = vec![[0.0;3];n_mu];
    let result = loop{
        class_result = cvt_classification(grids, &c_mu);
        println!("Step {:?}:", &count);
        ind_r = class_result.0;
        dist_r = class_result.1;
        let mut init_dist_r = 0.0;
        dist_r.iter().zip(lambda_r.iter()).for_each(|(d, w)|{
            init_dist_r += w * d * d;
        });
        dist = init_dist_r.sqrt();
        
        println!("    Initial distance_r = {:?}.", dist);
        

        let mut c_mu_new = cvt_update_cmu(&grids, &c_mu, &ind_r);
        let mut sum = 0.0;
        if count == 0 {
            //println!{"first ind_r is: {:?}", &ind_r};
        }
        if count == max_iter - 1 {
            c_mu_new.iter().zip(c_mu.iter()).for_each(|([x,y,z],[a,b,c])|{
                sum += ((x-a) * (x-a) + (y-b) * (y-b) + (z-c) * (z-c)).sqrt();
            });
            c_mu = c_mu_new;
            let ind_mu = cvt_find_corresponding_point(&grids, &c_mu, &ind_r);
            break (ind_mu, sum);
        } 

        let mut criterion = 0.0;
        c_mu.iter().for_each(|[x,y,z]|{
            criterion += 1e-6 * ((x * x + y * y + z * z).sqrt())
        });
        c_mu_new.iter().zip(c_mu.iter()).for_each(|([x,y,z],[a,b,c])|{
            sum += ((x-a) * (x-a) + (y-b) * (y-b) + (z-c) * (z-c)).sqrt();
        });
        
        if sum <= criterion{
            c_mu = c_mu_new;
            let ind_mu = cvt_find_corresponding_point(&grids, &c_mu, &ind_r);
            println!("Random points converged after {} iterations.", &count-1);
            println!("Dist: {:?}", sum);
            break (ind_mu, sum);
        }else {
            c_mu = c_mu_new;
            count += 1;
        }

        

    };

    result
}

pub fn prod_states_gw (phi: &MatrixFull<f64>, psi: &MatrixFull<f64>) -> MatrixFull<f64>{
    //Generate P: n_1n_2 \times n_r, P_{ij}(r) = \phi_i(r)\psi_j(r)
    //psi, phi: two wfn matrix
    //tested, maybe similar to otimes
    if phi.size[0] != psi.size[0]{
        panic!("Wrong inputs: row dimensions of Phi and Psi do not match!\n");
    }
    
    let mut prod = MatrixFull::new([phi.size[0], phi.size[1]*psi.size[1]],0.0);
    for i in 0..phi.size[1]{
        for j in 0..psi.size[1]{
            for k in 0..phi.size[0]{
                prod.data[k+phi.size[0]*(j+i*psi.size[1])] = phi.data[k+i*phi.size[0]] * psi.data[k+j*phi.size[0]];
            }
        }
    }
    prod
}

pub fn fourcenter_after_isdf(k_mu: usize, mol: &Molecule, grids: &dft::Grids) -> MatrixFull<f64> {
    let nao = mol.num_basis;
    let nri = mol.num_auxbas;
    let ngrids = grids.weights.len();
    let rgrid = &grids.coordinates;
    let lambda_r = &grids.weights;
    let mut phi = grids.ao.clone().unwrap(); // nao*ngrids
    //&phi.formated_output_e(5, "full");
    /*let mut rho = vec![0.0;ngrids];w
     rho.iter_mut().zip(phi.iter_columns_full()).for_each(|(sum,ao_on_grids)|{
        let mut sum_1 = 0.0;
        ao_on_grids.iter().for_each(|x|{
            sum_1 += x * x;  
        });
        *sum = sum_1;
    }); */
    let n_mu = k_mu * nri;
    let (ind_mu, loss_function) = cvt_isdf(&grids, n_mu);
    //println!("ind_mu: {:?}", &ind_mu);
    // get auxiliary basis zeta_mu
    let mut lambda_phi = MatrixFull::new([nao,ngrids],0.0);
    lambda_phi.iter_columns_full_mut().zip(lambda_r.iter().zip(phi.iter_columns_full()))
    .for_each(|(x, (weight,aos))|{
        x.iter_mut().zip(aos.iter()).for_each(|(y,ao)|{
            *y = weight * ao;
        });
    });
    &lambda_phi.formated_output_e(5, "full");
    let mut varphi = MatrixFull::new([nao,ind_mu.len()],0.0);
    varphi.iter_columns_full_mut().zip(ind_mu.iter()).for_each(|(new_phi,index)|{
        new_phi.iter_mut().zip(phi.iter_column(*index)).for_each(|(new_ao,ao)|{
            *new_ao = *ao;
        });
    });
    //&varphi.formated_output_e(5, "full");

    let mut lambda_varphi = MatrixFull::new([nao,ind_mu.len()],0.0);;
    lambda_varphi.iter_columns_full_mut().zip(ind_mu.iter()).for_each(|(new_lambda,index)|{
        new_lambda.iter_mut().zip(lambda_phi.iter_column(*index)).for_each(|(new_ao,ao)|{
            *new_ao = *ao;
        });
    });
    //&lambda_varphi.formated_output_e(5, "full");
    //C1 = (lambda_phi.T \cdot lambda_varphi) \times (phi.T \cdot varphi.T)  \times: Hadmard
    //C2 = (lambda_varphi.T \cdot lambda_varphi) \times (varphi.T \cdot varphi)
    let mut c11 = MatrixFull::new([ngrids, ind_mu.len()], 0.0);
    c11.lapack_dgemm(&mut lambda_phi, &mut lambda_varphi, 'T', 'N', 1.0, 0.0);
    let mut c12 = MatrixFull::new([ngrids, ind_mu.len()], 0.0);
    c12.lapack_dgemm(&mut phi, &mut varphi, 'T', 'N', 1.0, 0.0);
    let mut c1 = MatrixFull::new([ngrids, ind_mu.len()], 0.0);
    c1.iter_columns_full_mut().zip(c11.iter_columns_full()).zip(c12.iter_columns_full())
        .for_each(|((x,a),b)|{
            x.iter_mut().zip(a.iter()).zip(b.iter()).for_each(|((x1,a1),b1)|{
                *x1 = a1 *b1;
            });
        });

    let mut c21 = MatrixFull::new([ind_mu.len(), ind_mu.len()], 0.0);
    let mut lambda_varphi_mid = lambda_varphi.clone();
    c21.lapack_dgemm(&mut lambda_varphi, &mut lambda_varphi_mid, 'T', 'N', 1.0, 0.0);
    let mut c22 = MatrixFull::new([ind_mu.len(), ind_mu.len()], 0.0);
    let mut varphi_mid = varphi.clone();
    c22.lapack_dgemm(&mut varphi, &mut varphi_mid, 'T', 'N', 1.0, 0.0);
    let mut c2 = MatrixFull::new([ind_mu.len(), ind_mu.len()], 0.0);
    c2.iter_columns_full_mut().zip(c21.iter_columns_full()).zip(c22.iter_columns_full())
        .for_each(|((x,a),b)|{
            x.iter_mut().zip(a.iter()).zip(b.iter()).for_each(|((x1,a1),b1)|{
                *x1 = a1 * b1;
            });
        });
    //&c2.formated_output_e(5, "full");

    let mut inv_c2 = c2.lapack_inverse().unwrap();
    
    let mut zeta_mu = MatrixFull::new([ngrids,ind_mu.len()],0.0);
    zeta_mu.lapack_dgemm(&mut c1, &mut c2, 'N', 'N', 1.0, 0.0); //ISDF auxiliary wavefunction

    //<Z|V|P><P|V|P><P|V|Z>
    let mut cint_data = mol.initialize_cint();
    let n_basis_shell = mol.cint_bas.len();
    let n_auxbas_shell = mol.cint_aux_bas.len();
    let mut ri3fn = RIFull::new([nao,nao,nri],0.0);
    cint_data.cint2c2e_optimizer_rust();
    let mut ri_v_ri = MatrixFull::new([nri,nri],0.0);
    for l in 0..n_auxbas_shell {
        let basis_start_l = mol.cint_aux_fdqc[l][0];
        let basis_len_l = mol.cint_aux_fdqc[l][1];
        let gl  = l + n_basis_shell;
        for k in 0..n_auxbas_shell {
            let basis_start_k = mol.cint_aux_fdqc[k][0];
            let basis_len_k = mol.cint_aux_fdqc[k][1];
            let gk  = k + n_basis_shell;
            let buf = cint_data.cint_2c2e(gk as i32, gl as i32);
            
            let mut tmp_slices = ri_v_ri.iter_submatrix_mut(
                basis_start_k..basis_start_k+basis_len_k,
                basis_start_l..basis_start_l+basis_len_l);
            tmp_slices.zip(buf.iter()).for_each(|value| {*value.0 = *value.1});

        }
    }
    cint_data.cint3c2e_optimizer_rust();
    for k in 0..n_auxbas_shell {
        let basis_start_k = mol.cint_aux_fdqc[k][0];
        let basis_len_k = mol.cint_aux_fdqc[k][1];
        let gk  = k + n_basis_shell;
        for j in 0..n_basis_shell {
            let basis_start_j = mol.cint_fdqc[j][0];
            let basis_len_j = mol.cint_fdqc[j][1];
            // can be optimized with "for i in 0..(j+1)"
            for i in 0..n_basis_shell {
                let basis_start_i = mol.cint_fdqc[i][0];
                let basis_len_i = mol.cint_fdqc[i][1];
                let buf = RIFull::from_vec([basis_len_i, basis_len_j,basis_len_k], 
                    cint_data.cint_3c2e(i as i32, j as i32, gk as i32)).unwrap();
                ri3fn.copy_from_ri(
                    basis_start_i..basis_start_i+basis_len_i,
                    basis_start_j..basis_start_j+basis_len_j,
                    basis_start_k..basis_start_k+basis_len_k,
                    & buf, 
                    0..basis_len_i, 
                    0..basis_len_j, 
                    0..basis_len_k);
            }
        }
    }
        cint_data.final_c2r();
    let mut ri_v_ao = MatrixFull::new([nri, nao*nao],0.0);
    for i in 0..nri{
        for j in 0..nao*nao{
            ri_v_ao.data[j * nri + i] = ri3fn.data[i*nao*nao+i];
        }
    }

    let mut c = prod_states_gw(&lambda_varphi.transpose(), &varphi.transpose());
    let mut tmp1 = MatrixFull::new([nri, ind_mu.len()],0.0);
    tmp1.lapack_dgemm(&mut ri_v_ao, &mut c, 'T', 'N', 1.0, 0.0);
    let mut tmp0 = MatrixFull::new([nri,ind_mu.len()],0.0);
    tmp0.lapack_dgemm(&mut tmp1, &mut inv_c2, 'N', 'N', 1.0, 0.0);

    //lapack_dgesv
    let mut tmp = ri_v_ri.lapack_dgesv(&mut tmp0, nri as i32);
    let mut kernel_part = MatrixFull::new([ind_mu.len(),ind_mu.len()], 0.0);
    kernel_part.lapack_dgemm(&mut tmp0, &mut tmp, 'T', 'N', 1.0, 0.0);

    // generate result
    let mut mid = MatrixFull::new([nao*nao,ind_mu.len()],0.0);
    mid.lapack_dgemm(&mut c, &mut kernel_part, 'T', 'N', 1.0, 0.0);
    let mut fourcenter_after_isdf = MatrixFull::new([nao*nao,nao*nao],0.0);
    fourcenter_after_isdf.lapack_dgemm(&mut mid, &mut c, 'N', 'N', 1.0, 0.0);
    
    fourcenter_after_isdf
}


pub fn error_isdf(k_mu: Range<usize>, scf_data: &SCF) -> (Vec<usize>,Vec<f64>, Vec<f64>){
    let length = k_mu.len();
    let mut k_mu_list = vec![0 as usize; length];
    let mut abs_error_isdf = vec![0.0; length];
    let mut rel_error_isdf = vec![0.0; length];
    let mol = &scf_data.mol;
    let grids = scf_data.grids.as_ref().unwrap();
    // int2e
    let mut cint_data = mol.initialize_cint();
        let nbas = mol.num_basis;
        let mut mat_full = 
            ERIFull::new([nbas,nbas,nbas,nbas],0.0);
        let nbas_shell = mol.cint_bas.len();
        cint_data.cint2e_optimizer_rust();
        for l in 0..nbas_shell {
            let bas_start_l = mol.cint_fdqc[l][0];
            let bas_len_l = mol.cint_fdqc[l][1];
            for k in 0..(l+1) {
                let bas_start_k = mol.cint_fdqc[k][0];
                let bas_len_k = mol.cint_fdqc[k][1];
                for j in 0..nbas_shell {
                    let bas_start_j = mol.cint_fdqc[j][0];
                    let bas_len_j = mol.cint_fdqc[j][1];
                    //let (i_start, i_end) = (0,j+1);
                    for i in 0..j+1 {
                        let bas_start_i = mol.cint_fdqc[i][0];
                        let bas_len_i = mol.cint_fdqc[i][1];
                        let buf = cint_data.cint_ijkl_by_shell(i as i32, j as i32, k as i32, l as i32);
                        //let dt_cint_0 = time::Local::now();
                        //let dt_cint_1 = time::Local::now();
                        mat_full.chrunk_copy([bas_start_i..bas_start_i+bas_len_i,
                                              bas_start_j..bas_start_j+bas_len_j,
                                              bas_start_k..bas_start_k+bas_len_k,
                                              bas_start_l..bas_start_l+bas_len_l,
                                              ], buf.clone());
                        // copy the "upper" part to the lower part
                        if i<j {
                            mat_full.chrunk_copy_transpose_ij([
                                bas_start_i..bas_start_i+bas_len_i,
                                bas_start_j..bas_start_j+bas_len_j,
                                bas_start_k..bas_start_k+bas_len_k,
                                bas_start_l..bas_start_l+bas_len_l,
                                ], buf);
                        }
                    };
                }
            }
        }
        cint_data.final_c2r();
        // to copy the upper part of the (k,l) pair to the lower block
        for k in 0..nbas {
            for l in 0..k {
                let from_slice =  mat_full.get4d_slice([0,0,l,k], mat_full.indicing[2]).unwrap().to_vec();
                let mut to_slice = mat_full.get4d_slice_mut([0,0,k,l], mat_full.indicing[2]).unwrap();
                to_slice.iter_mut().zip(from_slice.iter()).for_each(|(t,f)|*t = *f);
            }
        }

        let mut fourcenter_before_isdf = MatrixFull::from_vec([nbas*nbas, nbas*nbas],mat_full.data).unwrap();
        let mut value = 0.0;
        fourcenter_before_isdf.iter().for_each(|x|{
            value += x * x;
        });
        let mut k = 0usize;
        for i in k_mu{
            let mut fourcenter_after_isdf = fourcenter_after_isdf(i, &mol, &grids);
            let mut abs_error = 0.0;
            fourcenter_after_isdf.data.iter().zip(fourcenter_before_isdf.iter()).for_each(|(after,before)|{
                abs_error += (after - before) * (after - before);
            });
            abs_error_isdf[k] = abs_error;
            rel_error_isdf[k] = abs_error/value;
            k_mu_list[k] = i;
            k += 1;
        }

    (k_mu_list, abs_error_isdf, rel_error_isdf)
}

pub fn index_of_min(nets: &mut Vec<f64>) -> usize{
    let index_of_min: Option<usize> = nets
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(Ordering::Equal))
        .map(|(index, _)| index);
    index_of_min.unwrap()
}