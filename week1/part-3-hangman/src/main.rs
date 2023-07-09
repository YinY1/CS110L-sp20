// Simple Hangman Program
// User gets five incorrect guesses
// Word chosen randomly from words.txt
// Inspiration from: https://doc.rust-lang.org/book/ch02-00-guessing-game-tutorial.html
// This assignment will introduce you to some fundamental syntax in Rust:
// - variable declaration
// - string manipulation
// - conditional statements
// - loops
// - vectors
// - files
// - user input
// We've tried to limit/hide Rust's quirks since we'll discuss those details
// more in depth in the coming lectures.
extern crate rand;
use rand::Rng;
use std::fs;
use std::io;
use std::io::Write;
use std::iter::FromIterator;
use std::vec;

const NUM_INCORRECT_GUESSES: u32 = 5;
const WORDS_PATH: &str = "words.txt";

fn pick_a_random_word() -> String {
    let file_string = fs::read_to_string(WORDS_PATH).expect("Unable to read file.");
    let words: Vec<&str> = file_string.split('\n').collect();
    String::from(words[rand::thread_rng().gen_range(0, words.len())].trim())
}

fn main() {
    let secret_word = pick_a_random_word();
    // Note: given what you know about Rust so far, it's easier to pull characters out of a
    // vector than it is to pull them out of a string. You can get the ith character of
    // secret_word by doing secret_word_chars[i].
    let secret_word_chars: Vec<char> = secret_word.chars().collect();
    // Uncomment for debugging:
    // println!("random word: {}", secret_word);

    // Your code here! :)
    println!("Welcome to CS110L Hangman!");

    let mut chances = NUM_INCORRECT_GUESSES;
    let mut rest = secret_word_chars.len();
    let mut guessed: Vec<char> = Vec::new();
    let mut correct: Vec<char> = vec!['-'; rest];
    while chances > 0 && rest > 0 {
        let letter = do_guess(chances, &mut guessed, &correct);
        if !check_guess(letter, &mut correct, &secret_word_chars) {
            chances -= 1;
        } else {
            rest -= 1;
        }
        println!();
    }
    if chances > 0 {
        println!(
            "Congratulations you guessed the secret word: {}!",
            secret_word
        );
    } else {
        println!("Sorry, you ran out of guesses! ");
    }
}

fn do_guess(chances: u32, guess: &mut Vec<char>, correct: &[char]) -> char {
    println!("The word so far is {}", String::from_iter(correct.iter()));
    println!(
        "You have guessed the following letters: {}",
        String::from_iter(guess.iter())
    );
    println!("You have {chances} guesses left");
    print!("Please guess a letter: ");

    io::stdout().flush().expect("Error flushing stdout.");

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("Error reading line.");

    let letter: char = input.chars().next().unwrap();
    guess.push(letter);

    letter
}

fn check_guess(letter: char, correct: &mut [char], secret: &Vec<char>) -> bool {
    let mut i = 0;
    while i < secret.len() {
        let c = secret[i];
        if c == letter && correct[i] == '-' {
            correct[i] = c;
            return true;
        }
        i += 1;
    }
    println!("Sorry, that letter is not in the word");

    false
}
