use std::collections::VecDeque;
#[allow(unused_imports)]
use std::sync::{Arc, Mutex};
use std::time::Instant;
#[allow(unused_imports)]
use std::{env, process, thread};

/// Determines whether a number is prime. This function is taken from CS 110 factor.py.
///
/// You don't need to read or understand this code.
#[allow(dead_code)]
fn is_prime(num: u32) -> bool {
    if num <= 1 {
        return false;
    }
    for factor in 2..((num as f64).sqrt().floor() as u32) {
        if num % factor == 0 {
            return false;
        }
    }
    true
}

/// Determines the prime factors of a number and prints them to stdout. This function is taken
/// from CS 110 factor.py.
///
/// You don't need to read or understand this code.
#[allow(dead_code)]
fn factor_number(num: u32) {
    let start = Instant::now();

    if num == 1 || is_prime(num) {
        println!("{} = {} [time: {:?}]", num, num, start.elapsed());
        return;
    }

    let mut factors = Vec::new();
    let mut curr_num = num;
    for factor in 2..num {
        while curr_num % factor == 0 {
            factors.push(factor);
            curr_num /= factor;
        }
    }
    factors.sort();
    let factors_str = factors
        .into_iter()
        .map(|f| f.to_string())
        .collect::<Vec<String>>()
        .join(" * ");
    println!("{} = {} [time: {:?}]", num, factors_str, start.elapsed());
}

/// Returns a list of numbers supplied via argv.
#[allow(dead_code)]
fn get_input_numbers() -> VecDeque<u32> {
    let mut numbers = VecDeque::new();
    for arg in env::args().skip(1) {
        if let Ok(val) = arg.parse::<u32>() {
            numbers.push_back(val);
        } else {
            println!("{} is not a valid number", arg);
            process::exit(1);
        }
    }
    numbers
}

fn main() {
    let num_threads = num_cpus::get();
    println!("Farm starting on {} CPUs", num_threads);
    let start = Instant::now();

    let number_queue = Arc::new(Mutex::new(get_input_numbers()));

    // factor_number() until the queue is empty
    let mut threads = Vec::new();
    for _ in 1..num_threads {
        let handle = number_queue.clone();
        threads.push(thread::spawn(move || {
            factor_agent(handle);
        }))
    }

    for thread in threads {
        thread.join().expect("Panic occurred in thread!");
    }

    println!("Total execution time: {:?}", start.elapsed());
}

fn factor_agent(number_queue: Arc<Mutex<VecDeque<u32>>>) {
    while let Some(number) = get_factor_number(&number_queue) {
        factor_number(number);
    }
}

fn get_factor_number(number_queue: &Arc<Mutex<VecDeque<u32>>>) -> Option<u32> {
    let mut queue_ref = number_queue.lock().unwrap();
    if (*queue_ref).is_empty() {
        return None;
    }
    (*queue_ref).pop_front()
}
