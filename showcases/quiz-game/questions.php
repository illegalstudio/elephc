<?php
// Question bank
// Each question is: [text, optA, optB, optC, optD, answer]
// All values are strings in indexed array to avoid mixed assoc types

function get_questions() {
    return [
        ["What year was the C programming language created?", "1965", "1972", "1980", "1991", "b"],
        ["Which planet has the most moons?", "Jupiter", "Saturn", "Neptune", "Uranus", "b"],
        ["What does CPU stand for?", "Central Process Unit", "Central Processing Unit", "Computer Personal Unit", "Central Processor Utility", "b"],
        ["Who created Linux?", "Richard Stallman", "Dennis Ritchie", "Linus Torvalds", "Ken Thompson", "c"],
        ["What is the smallest prime number?", "0", "1", "2", "3", "c"],
        ["In what year did PHP first appear?", "1991", "1995", "1998", "2000", "b"],
        ["What is 2^10?", "512", "1000", "1024", "2048", "c"],
        ["Which data structure uses FIFO ordering?", "Stack", "Queue", "Tree", "Graph", "b"],
        ["What is the time complexity of binary search?", "O(n)", "O(n log n)", "O(log n)", "O(1)", "c"],
        ["How many bits are in a byte?", "4", "8", "16", "32", "b"],
        ["Which animal is the mascot of the PHP language?", "A snake", "An elephant", "A penguin", "A gopher", "b"],
        ["What does HTML stand for?", "Hyper Text Markup Language", "High Tech Modern Language", "Hyper Transfer Markup Language", "Home Tool Markup Language", "a"],
    ];
}
