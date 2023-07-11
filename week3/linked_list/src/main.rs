use linked_list::LinkedList;

use crate::linked_list::ComputeNorm;
pub mod linked_list;

fn main() {
    let mut list: LinkedList<u32> = LinkedList::new();
    assert!(list.is_empty());
    assert_eq!(list.get_size(), 0);
    for i in 1..12 {
        list.push_front(i);
    }
    println!("{}", list);
    println!("list size: {}", list.get_size());
    println!("top element: {}", list.pop_front().unwrap());
    println!("{}", list);
    println!("size: {}", list.get_size());
    println!("{}", list.to_string()); // ToString impl for anything impl Display

    let mut list2 = list.clone();
    println!("list == list2: {}", list == list2);
    println!("list:{}\nlist2:{}",list,list2);
    list2.pop_front();
    println!("list != list2: {}", list != list2);

    let mut list3 = list2.clone();
    list3.pop_front();
    println!("list != list3: {}", list != list3);
    println!("list:{}\nlist3:{}",list,list3);

    let mut list4: LinkedList<u32> = LinkedList::new();
    for i in 1..12 {
        list4.push_front(i);
    }
    list4.pop_front();
    println!("list == list4: {}", list == list4);
    println!("list:{}\nlist4:{}",list,list4);
    
    //If you implement iterator trait:
    for val in &list {
        println!("{}", val);
    }

    let mut listf:LinkedList<f64> = LinkedList::new();
    for _ in 1..12{
        listf.push_front(1f64);
    }
    println!("compute norm: {}",listf.compute_norm());
    
}
