use std::{thread, time};

fn parallel_map<T, U, F>(mut input_vec: Vec<T>, num_threads: usize, f: F) -> Vec<U>
where
    F: FnOnce(T) -> U + Send + Copy + 'static,
    T: Send + 'static,
    U: Send + 'static + Default,
{
    let mut output_vec: Vec<U> = Vec::with_capacity(input_vec.len());

    for _ in 0..input_vec.len() {
        output_vec.push(Default::default());
    }
    let (sender_raw, receiver_raw) = crossbeam_channel::unbounded();
    let (sender_result, receiver_result) = crossbeam_channel::unbounded();
    let mut threads = vec![];
    for _ in 0..num_threads {
        let recv_raw = receiver_raw.clone();
        let send_res = sender_result.clone();
        threads.push(thread::spawn(move || {
            while let Ok((num, idx)) = recv_raw.recv() {
                send_res
                    .send((f(num), idx))
                    .expect("Tried writing to channel, but there are no receivers!");
            }
        }));
    }

    drop(sender_result);

    let mut i = input_vec.len();
    while let Some(num) = input_vec.pop() {
        i -= 1;
        sender_raw
            .send((num, i))
            .expect("Tried writing to channel, but there are no receivers!");
    }

    drop(sender_raw);

    while let Ok((res, idx)) = receiver_result.recv() {
        output_vec[idx] = res;
    }

    for thread in threads {
        thread.join().expect("Panic occurred in thread");
    }
    output_vec
}

fn main() {
    let v = vec![6, 7, 8, 9, 10, 1, 2, 3, 4, 5, 12, 18, 11, 5, 20];
    let squares = parallel_map(v, 10, |num| {
        println!("{} squared is {}", num, num * num);
        thread::sleep(time::Duration::from_millis(500));
        num * num
    });
    println!("squares: {:?}", squares);
}
