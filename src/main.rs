pub mod bots;

use crate::bots::*;

fn main() {
    flaud::flaud_test();
    rnetr::rnetr_test();
}