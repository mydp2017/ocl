//! This is more of a test than anything but put here as an example of how
//! you could use custom event callbacks.
//!
//! 

extern crate libc;
extern crate ocl;

use libc::c_void;
use ocl::{util, core, ProQue, Program, Buffer, EventList};
// use ocl::traits::{BufferExtras};
use ocl::cl_h::{cl_event, cl_int};

// How many iterations we wish to run:
const ITERATIONS: usize = 8;
// Whether or not to print:
const PRINT_DEBUG: bool = true;
// How many results to print from each iteration:
const RESULTS_TO_PRINT: usize = 5;

struct TestEventsStuff {
    seed_env: *const Buffer<u32>, 
    res_env: *const Buffer<u32>, 
    data_set_size: usize,
    addend: u32, 
    itr: usize,
}

fn main() {
    // Create a context, program, & queue: 
    let ocl_pq = ProQue::builder()
        .prog_bldr(Program::builder().src_file("cl/kernel_file.cl"))
        .build().unwrap();

    // Set up data set size and work dimensions:
    let dims = [900000];

    // Create source and result buffers (our data containers):
    // let seed_buffer = Buffer::with_vec_scrambled((0u32, 500u32), &dims, &ocl_pq.queue());
    let vec_seed = util::scrambled_vec((0u32, 500u32), ocl_pq.dims().to_len().unwrap());
    let seed_buffer = Buffer::newer_new(ocl_pq.queue(), Some(core::MEM_READ_WRITE | 
        core::MEM_COPY_HOST_PTR), ocl_pq.dims().clone(), Some(&vec_seed)).unwrap();

    // let mut result_buffer = Buffer::with_vec(&dims, &ocl_pq.queue());
    let mut vec_result = vec![0.0f32; dims[0]];
    let mut result_buffer = Buffer::<f32>::newer_new(ocl_pq.queue(), None, 
        ocl_pq.dims(), None).unwrap();

    // Our arbitrary addend:
    let addend = 11u32;

    // Create kernel with the source initially set to our seed values.
    let mut kernel = ocl_pq.create_kernel("add_scalar")
        .gws(&dims)
        .arg_buf_named("src", Some(&seed_buffer))
        .arg_scl(addend)
        .arg_buf(&mut result_buffer);

    // Create event list:
    let mut kernel_event = EventList::new();    

    //#############################################################################################

    // Create storage for per-event data:
    let mut buncha_stuffs = Vec::<TestEventsStuff>::with_capacity(ITERATIONS);

    // Run our test:
    for itr in 0..ITERATIONS {
        // Store information for use by the result callback function into a vector
        // which will persist until all of the commands have completed (as long as
        // we are sure to allow the queue to finish before returning).
        buncha_stuffs.push(TestEventsStuff {
            seed_env: &seed_buffer as *const Buffer<u32>,
            res_env: &result_buffer as *const Buffer<u32>, 
            data_set_size: dims[0], 
            addend: addend, 
            itr: itr,
        });

        // Change the source buffer to the result after seed values have been copied.
        // Yes, this is far from optimal...
        // Should just copy the values in the first place but oh well.
        if itr != 0 {
            kernel.set_arg_buf_named("src", Some(&result_buffer)).unwrap();
        }

        if PRINT_DEBUG { println!("Enqueuing kernel [itr:{}]...", itr); }
        kernel.cmd().enew(&mut kernel_event).enq().unwrap();

        let mut read_event = EventList::new();
        
        if PRINT_DEBUG { println!("Enqueuing read buffer [itr:{}]...", itr); }
        // unsafe { result_buffer.enqueue_fill_vec(false, None, Some(&mut read_event)).unwrap(); }
        unsafe { result_buffer.cmd().read_async(&mut vec_result)
            .enew(&mut read_event).enq().unwrap(); }
    
        // Clone event list just for fun:
        let read_event = read_event.clone();

        let last_idx = buncha_stuffs.len() - 1;     

        unsafe {
            if PRINT_DEBUG { println!("Setting callback (verify_result, buncha_stuff[{}]) [i:{}]...", 
                last_idx, itr); }
            read_event.set_callback(Some(_test_events_verify_result), 
                // &mut buncha_stuffs[last_idx] as *mut _ as *mut c_void);
                &mut buncha_stuffs[last_idx]).unwrap();
        }

        // if PRINT_DEBUG { println!("Releasing read_event [i:{}]...", itr); }
        // // Decrement reference count. Will still complete before releasing.
        // read_event.release_all();
    }

    // Wait for all queued tasks to finish so that verify_result() will be called:
    ocl_pq.queue().finish();
}



// Callback for `test_events()`.
//
// Yeah it's ugly.
extern fn _test_events_verify_result(event: cl_event, status: cl_int, user_data: *mut c_void) {
    let buncha_stuff = user_data as *const TestEventsStuff;    

    unsafe {
        let seed_buffer: *const Buffer<u32> = (*buncha_stuff).seed_env as *const Buffer<u32>;
        let result_buffer: *const Buffer<u32> = (*buncha_stuff).res_env as *const Buffer<u32>;
        let data_set_size: usize = (*buncha_stuff).data_set_size;
        let addend: u32 = (*buncha_stuff).addend;
        let itr: usize = (*buncha_stuff).itr;
        
        if PRINT_DEBUG { println!("\nEvent: `{:?}` has completed with status: `{}`, data_set_size: '{}`, \
                 addend: {}, itr: `{}`.", event, status, data_set_size, addend, itr); }

        for idx in 0..data_set_size {
            assert_eq!((*result_buffer)[idx], 
                ((*seed_buffer)[idx] + ((itr + 1) as u32) * addend));

            if PRINT_DEBUG && (idx < RESULTS_TO_PRINT) {
                let correct_result = (*seed_buffer)[idx] + (((itr + 1) as u32) * addend);
                print!("correct_result: {}, result_buffer[{idx}]:{}\n",
                    correct_result, (*result_buffer)[idx], idx = idx);
            }
        }

        let mut errors_found = 0;

        for idx in 0..data_set_size {
            // [FIXME]: FAILING ON OSX -- TEMPORARLY COMMENTING OUT
            // assert_eq!((*result_buffer)[idx], 
            //  ((*seed_buffer)[idx] + ((itr + 1) as u32) * addend));

            if PRINT_DEBUG {
                let correct_result = (*seed_buffer)[idx] + (((itr + 1) as u32) * addend);

                if (*result_buffer)[idx] != correct_result {
                    print!("correct_result:{}, result_buffer[{idx}]:{}\n",
                        correct_result, (*result_buffer)[idx], idx = idx);

                    errors_found += 1;
                }
            }
        }

        if PRINT_DEBUG { 
            if errors_found > 0 { print!("TOTAL ERRORS FOUND: {}\n", errors_found); }
        }
    }
}