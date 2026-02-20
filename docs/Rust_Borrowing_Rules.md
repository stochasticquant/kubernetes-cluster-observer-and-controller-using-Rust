# Rust Borrowing Rules: A Complete Guide

## Table of Contents
1. [Introduction](#introduction)
2. [Ownership](#ownership)
3. [Borrowing Basics](#borrowing-basics)
4. [The Borrowing Rules](#the-borrowing-rules)
5. [Immutable References](#immutable-references)
6. [Mutable References](#mutable-references)
7. [Understanding Lifetimes](#understanding-lifetimes)
8. [Common Borrowing Errors](#common-borrowing-errors)
9. [Practical Examples](#practical-examples)
10. [Best Practices](#best-practices)

---

## Introduction

Rust's borrowing system is one of its most powerful and unique features. It enables **memory safety without a garbage collector** by enforcing strict rules at compile time. Understanding borrowing is crucial to writing Rust code that compiles and runs correctly.

The core idea: **You can own data or borrow it, but not both at the same time in the same scope.**

---

## Ownership

Before understanding borrowing, you need to understand ownership.

### The Three Ownership Rules

1. **Each value has one owner**
2. **When the owner goes out of scope, the value is dropped (deallocated)**
3. **Ownership can be transferred (moved) to another variable**

### Example: Ownership in Action

```rust
fn main() {
    let s1 = String::from("hello");  // s1 owns the string
    let s2 = s1;                     // Ownership moves to s2
    
    // println!("{}", s1);  // ❌ ERROR! s1 no longer owns the data
    println!("{}", s2);    // ✅ OK! s2 is the owner now
}
// When main() ends, s2 goes out of scope and the string is deallocated
```

**What happens:**
- `s1` initially owns the `String` "hello"
- `s2 = s1` **moves** ownership from s1 to s2
- s1 is now invalid (we can't use it)
- When the scope ends, only s2's destructor runs, deallocating the memory once

### Stack vs Heap

This behavior differs for small, fixed-size types:

```rust
fn main() {
    let x = 5;       // x owns the integer 5
    let y = x;       // 5 is copied to y (integers are Copy types)
    
    println!("{}", x);  // ✅ OK! x is still valid
    println!("{}", y);  // ✅ OK! y is valid too
}
```

**Why the difference?**
- Integers implement the `Copy` trait
- `Copy` types are duplicated instead of moved
- Small, stack-allocated types are `Copy`
- Heap-allocated types like `String` are not `Copy` (moving is cheaper than copying)

---

## Borrowing Basics

Instead of transferring ownership, you can **borrow** data. Borrowing lets you use data without taking ownership.

### Two Types of Borrowing

1. **Immutable borrowing** (`&T`): Read-only access
2. **Mutable borrowing** (`&mut T`): Read and write access

### The Core Borrowing Rule

> At any given time, you can have **either** one mutable reference **or** multiple immutable references, **but not both**.

This rule prevents data races and use-after-free errors.

---

## The Borrowing Rules

### Rule 1: One Mutable Reference OR Multiple Immutable References

You cannot have a mutable reference while immutable references exist:

```rust
fn main() {
    let mut s = String::from("hello");
    
    let r1 = &s;      // Immutable borrow #1
    let r2 = &s;      // Immutable borrow #2
    let r3 = &mut s;  // ❌ ERROR! Can't borrow as mutable while immutable borrows exist
    
    println!("{}, {}", r1, r2);
}
```

**Why this rule exists:**
If r3 had been allowed, it could modify the string while r1 and r2 still reference it, potentially causing memory corruption.

### Rule 2: Mutable References Are Exclusive

Only ONE mutable reference can exist at a time:

```rust
fn main() {
    let mut s = String::from("hello");
    
    let r1 = &mut s;
    let r2 = &mut s;  // ❌ ERROR! Only one mutable reference allowed
    
    println!("{}", r1);
}
```

**Why this rule exists:**
If both r1 and r2 could exist, both could modify the string simultaneously, creating a data race.

### Rule 3: References Must Not Outlive Their Owner

A reference becomes invalid when the value it points to is dropped:

```rust
fn main() {
    let r;
    {
        let s = String::from("hello");
        r = &s;  // r borrows s
    } // s is dropped here; r is now invalid
    
    println!("{}", r);  // ❌ ERROR! r is a dangling reference
}
```

**Rust term:** This is a **dangling reference**, and Rust prevents it at compile time.

---

## Immutable References

An immutable reference (`&T`) lets you read data without modifying it. You can have multiple immutable references.

### Basic Immutable Borrowing

```rust
fn main() {
    let s = String::from("hello");
    
    let r1 = &s;  // Borrow immutably
    let r2 = &s;  // Another immutable borrow (OK!)
    
    println!("{}", r1);  // ✅ OK
    println!("{}", r2);  // ✅ OK
    println!("{}", s);   // ✅ OK - original still accessible
}
```

**Lifetime visualization:**
```
┌─────────────────────────────────────┐
│ s = String::from("hello")           │
│                                     │
│ r1 = &s  (immutable borrow)        │
│ r2 = &s  (immutable borrow)        │
│                                     │
│ Use r1, r2, s (all valid)          │
│                                     │
└─────────────────────────────────────┘ // All dropped here
```

### Passing Immutable References to Functions

```rust
fn print_length(s: &String) {  // Takes an immutable reference
    println!("Length: {}", s.len());
}

fn main() {
    let s = String::from("hello");
    
    print_length(&s);  // Borrow s, don't move it
    print_length(&s);  // Can borrow again
    
    println!("{}", s);  // s is still valid
}
```

**Without borrowing, ownership would transfer:**
```rust
// ❌ This doesn't work:
fn print_length(s: String) {  // Takes ownership
    println!("Length: {}", s.len());
}

fn main() {
    let s = String::from("hello");
    print_length(s);   // s moves to function
    print_length(s);   // ❌ ERROR! s no longer owned by main
}
```

---

## Mutable References

A mutable reference (`&mut T`) lets you modify data, but only one mutable reference can exist at a time.

### Basic Mutable Borrowing

```rust
fn main() {
    let mut s = String::from("hello");
    
    let r = &mut s;  // Mutable borrow
    r.push_str(" world");
    
    println!("{}", r);  // ✅ Prints: hello world
    println!("{}", s);  // ✅ Prints: hello world (s was modified)
}
```

### Why Only One Mutable Reference?

```rust
fn main() {
    let mut s = String::from("hello");
    
    let r1 = &mut s;
    let r2 = &mut s;  // ❌ ERROR! Can't have two mutable references
    
    r1.push_str(" world");
    r2.push_str("!");
    println!("{}", r1);
}
```

**If this were allowed:**
- r1 and r2 both point to the same data
- r1 and r2 could execute concurrently
- Both could modify the same memory → data corruption!

### Passing Mutable References to Functions

```rust
fn add_exclamation(s: &mut String) {  // Takes a mutable reference
    s.push_str("!");
}

fn main() {
    let mut s = String::from("hello");
    
    add_exclamation(&mut s);  // Pass mutable reference
    println!("{}", s);        // ✅ Prints: hello!
}
```

### Scope of Mutable References

A mutable reference is only held while you're actively using it:

```rust
fn main() {
    let mut s = String::from("hello");
    
    let r1 = &mut s;
    r1.push_str(" world");
    println!("{}", r1);  // Last use of r1
    
    // r1's scope effectively ends here (last use was above)
    
    let r2 = &mut s;  // ✅ OK! r1 is no longer being used
    r2.push_str("!");
    println!("{}", r2);
}
```

This is called **Non-Lexical Lifetimes (NLL)**. The borrow ends when the reference is last used, not when it goes out of scope.

---

## Understanding Lifetimes

A **lifetime** is how long a reference is valid. Lifetimes ensure that references don't outlive the data they point to. Rust uses lifetime parameters to track this at compile time.

### What Are Lifetimes?

A lifetime is the scope during which a reference is valid. The Rust compiler uses lifetimes to ensure that:
1. References don't point to dropped data
2. Borrowed data outlives all references to it

**Key concept:** Lifetimes are about ensuring references don't become dangling.

```rust
fn main() {
    let x = 5;        // x begins here
    let r = &x;       // r is valid while x is alive
    println!("{}", r); // r is used here
}                      // Both x and r end here
```

### Lifetime Annotations Syntax

Lifetime parameters are declared with a single quote: `'a`, `'b`, `'lifetime`, etc.

```rust
// A reference with explicit lifetime
let r: &'a i32;

// A function parameter with lifetime
fn example(s: &'a String) { }

// Multiple lifetimes
fn combine(a: &'a String, b: &'b String) { }
```

**Important:** Lifetimes don't change how long data lives. They're just annotations for the compiler to verify references are valid.

### Elision: Implicit Lifetimes

Most of the time, you don't need to write lifetime annotations. Rust has **lifetime elision rules** that infer lifetimes automatically:

```rust
// With explicit lifetimes (verbose)
fn get_length<'a>(s: &'a String) -> usize {
    s.len()
}

// Without explicit lifetimes (elided)
fn get_length(s: &String) -> usize {
    s.len()
}
```

Both are equivalent. The compiler infers the lifetimes in the second version.

### When You Need Explicit Lifetimes

You must write explicit lifetime annotations when the compiler can't infer them. This typically happens when:
1. A function returns a reference
2. A function takes multiple reference parameters
3. Struct fields hold references

### Example 1: Function Returning a Reference

```rust
// ❌ ERROR! Compiler can't infer which input lifetime to use
fn get_first<'a>(a: &'a String, b: &'a String) -> &'a String {
    if a.len() > b.len() {
        a
    } else {
        b
    }
}

fn main() {
    let s1 = String::from("hello");
    let s2 = String::from("world");
    
    let result = get_first(&s1, &s2);
    println!("{}", result);  // ✅ result is valid as long as s1 and s2 are
}
```

**What the lifetime means:**
- `'a` is a lifetime parameter
- The function takes two string references both living for `'a`
- The function returns a reference that also lives for `'a`
- This guarantees the returned reference is valid as long as both inputs are valid

**Lifetime diagram:**
```
Input:    &'a s1 ─────────────┐
Input:    &'a s2 ─────────────┤
                              ├─→ Result lives for 'a
Return:   &'a String ◄────────┘

All valid in the same scope
```

### Example 2: Mutable Lifetimes

```rust
fn append<'a>(main: &'a mut String, text: &str) {
    main.push_str(text);
}

fn main() {
    let mut s = String::from("Hello");
    append(&mut s, " World");
    println!("{}", s);  // ✅ OK!
}
```

### Example 3: Different Input and Output Lifetimes

```rust
// The returned reference lives as long as the input string
fn get_first_word(s: &str) -> &str {
    let bytes = s.as_bytes();
    for (i, &item) in bytes.iter().enumerate() {
        if item == b' ' {
            return &s[..i];
        }
    }
    &s
}

fn main() {
    let sentence = String::from("Hello World");
    let word = get_first_word(&sentence);
    println!("{}", word);  // ✅ word lives as long as sentence
}
```

With explicit lifetimes:
```rust
fn get_first_word<'a>(s: &'a str) -> &'a str {
    let bytes = s.as_bytes();
    for (i, &item) in bytes.iter().enumerate() {
        if item == b' ' {
            return &s[..i];
        }
    }
    &s
}
```

### Example 4: Struct with References

When a struct holds a reference, you must annotate the lifetime:

```rust
// ❌ ERROR! Which value does the reference point to?
struct User {
    name: &String,
    age: u32,
}

// ✅ CORRECT! Reference lives as long as 'a
struct User<'a> {
    name: &'a String,
    age: u32,
}

fn main() {
    let name = String::from("Alice");
    let user = User {
        name: &name,
        age: 30,
    };
    
    println!("Name: {}", user.name);  // ✅ OK
}  // name and user dropped together
```

**Important:** The struct can't outlive the reference it holds.

```rust
fn create_user() -> User {  // ❌ ERROR!
    let name = String::from("Alice");
    User {
        name: &name,  // name is dropped at end of function!
        age: 30,
    }
}
```

This would create a dangling reference, so Rust prevents it.

### Example 5: Multiple Lifetimes

```rust
// Two different lifetimes for two different references
fn combine<'a, 'b>(s1: &'a str, s2: &'b str) -> String {
    format!("{} {}", s1, s2)
}

fn main() {
    let a = String::from("hello");
    let b = String::from("world");
    
    let result = combine(&a, &b);
    println!("{}", result);  // ✅ OK!
}
```

The lifetimes `'a` and `'b` are independent. They don't need to be the same duration.

### Example 6: Lifetime Bounds

You can specify that one lifetime must outlive another:

```rust
// 'a must live as long as 'b
fn example<'a, 'b: 'a>(a: &'a str, b: &'b str) {
    println!("{} {}", a, b);
}

fn main() {
    let b = String::from("outer");
    {
        let a = String::from("inner");
        example(&a, &b);  // ✅ OK! 'a outlives, 'b is outer
    }
}
```

### The Three Lifetime Elision Rules

Rust automatically applies these rules when you don't write explicit lifetimes:

**Rule 1:** Each input reference gets its own lifetime
```rust
fn example(s1: &str, s2: &str) // becomes: <'a, 'b>
```

**Rule 2:** If there's one input lifetime, it's assigned to the output
```rust
fn get_length(s: &str) -> usize // becomes: <'a>
```

**Rule 3:** If there's `&self` or `&mut self`, its lifetime is assigned to outputs
```rust
struct Thing;
impl Thing {
    fn get(&self) -> &i32 { }  // becomes: <'a> for &'a self
}
```

### Lifetime Variance: 'static

The `'static` lifetime means the reference is valid for the entire program duration:

```rust
// String literals are 'static
let s: &'static str = "hello";  // Valid for entire program

// Regular String references are not 'static
let s = String::from("hello");
let r: &String = &s;  // Some temporary lifetime, not 'static

// Function can accept any lifetime
fn print_any(s: &str) { }  // Accepts both 'static and temporary

// Function requires 'static
fn print_static(s: &'static str) { }  // Only string literals and similar

fn main() {
    print_any("hello");        // ✅ 'static string
    
    let s = String::from("hi");
    print_any(&s);             // ✅ Temporary lifetime
    
    print_static("hello");     // ✅ 'static string
    print_static(&s);          // ❌ ERROR! s is not 'static
}
```

### Lifetime Errors and Debugging

When you see a lifetime error, focus on:

```
error[E0597]: `name` does not live long enough
  --> src/main.rs:5:15
   |
5  |         name: &name,
   |               ^^^^^ borrowed value does not live long enough
...
10 | }
   | - `name` dropped here while still borrowed
```

**Translation:** A reference outlives its data.

**Solution:** Ensure the data lives as long as the reference.

### Practice: Reading Complex Lifetimes

```rust
fn process<'a, 'b: 'a>(input: &'a str, context: &'b str) -> &'a str
```

**Reading it:**
- Takes two references: `input` (lifetime `'a`) and `context` (lifetime `'b`)
- `'b: 'a` means `'b` outlives `'a`
- Returns a reference with lifetime `'a`
- The returned reference won't outlive `input`

### Lifetime Best Practices

1. **Let the compiler infer when possible**
   ```rust
   // Good: Let compiler infer
   fn example(s: &str) -> usize { s.len() }
   ```

2. **Use explicit lifetimes for clarity**
   ```rust
   // Better for complex functions
   fn combine<'a, 'b>(s1: &'a str, s2: &'b str) -> String { ... }
   ```

3. **Lifetime names should be descriptive when needed**
   ```rust
   // Instead of 'a, 'b, 'c, use meaningful names
   fn process<'request, 'cache>(req: &'request str, cache: &'cache str) { }
   ```

4. **Consider ownership over borrowing for simplicity**
   ```rust
   // Instead of juggling lifetimes:
   fn example(s: String) -> String { s }
   
   // Sometimes simpler than:
   fn example<'a>(s: &'a String) -> &'a str { &s }
   ```

---

## Common Borrowing Errors

### Error 1: Mutable and Immutable Borrows Together

```rust
fn main() {
    let mut s = String::from("hello");
    
    let r1 = &s;      // Immutable borrow
    let r2 = &s;      // Another immutable borrow
    let r3 = &mut s;  // ❌ ERROR! Can't mix with immutable borrows
    
    println!("{}, {}, {}", r1, r2, r3);
}
```

**Error message:**
```
error[E0502]: cannot borrow `s` as mutable because it is also borrowed as immutable
```

**Fix:**
```rust
fn main() {
    let mut s = String::from("hello");
    
    let r1 = &s;
    let r2 = &s;
    println!("{}, {}", r1, r2);  // Last use of immutable borrows
    
    let r3 = &mut s;  // ✅ OK! Immutable borrows are done
    r3.push_str("!");
    println!("{}", r3);
}
```

### Error 2: Multiple Mutable References

```rust
fn main() {
    let mut s = String::from("hello");
    
    let r1 = &mut s;
    let r2 = &mut s;  // ❌ ERROR!
    
    r1.push_str(" world");
    r2.push_str("!");
}
```

**Error message:**
```
error[E0499]: cannot borrow `s` as mutable more than once at a time
```

**Fix:**
```rust
fn main() {
    let mut s = String::from("hello");
    
    let r1 = &mut s;
    r1.push_str(" world");
    println!("{}", r1);
    // r1's scope ends here
    
    let r2 = &mut s;  // ✅ OK! Now r1 is no longer used
    r2.push_str("!");
    println!("{}", r2);
}
```

### Error 3: Dangling References

```rust
fn main() {
    let r;
    {
        let s = String::from("hello");
        r = &s;
    }  // s is dropped here
    
    println!("{}", r);  // ❌ ERROR! r points to dropped data
}
```

**Error message:**
```
error[E0597]: `s` does not live long enough
```

**Fix:**
Move the reference's scope inside the data's scope:
```rust
fn main() {
    let s = String::from("hello");
    let r = &s;  // Both r and s in same scope
    
    println!("{}", r);  // ✅ OK!
}  // Both dropped together
```

---

## Practical Examples

### Example 1: Borrowing in Collections

```rust
fn main() {
    let mut numbers = vec![1, 2, 3, 4, 5];
    
    // Multiple immutable borrows
    let first = &numbers[0];
    let second = &numbers[1];
    
    println!("First: {}, Second: {}", first, second);  // ✅ OK
    
    // Now we can use a mutable reference
    numbers.push(6);
    println!("{:?}", numbers);
}
```

### Example 2: Function Taking References

```rust
fn calculate_length(s: &String) -> usize {
    s.len()
}

fn append_text(s: &mut String, text: &str) {
    s.push_str(text);
}

fn main() {
    let mut message = String::from("Hello");
    
    let len = calculate_length(&message);
    println!("Length: {}", len);  // Prints: 5
    
    append_text(&mut message, " World");
    println!("{}", message);  // Prints: Hello World
}
```

### Example 3: Preventing Misuse

```rust
// Bad: Takes ownership for no reason
fn process_string_bad(s: String) {
    println!("{}", s);
}

// Good: Borrows instead
fn process_string_good(s: &String) {
    println!("{}", s);
}

fn main() {
    let msg = String::from("hello");
    
    // With borrowed version, we can use msg multiple times
    process_string_good(&msg);
    process_string_good(&msg);
    process_string_good(&msg);
    
    println!("{}", msg);  // Still valid!
}
```

### Example 4: Struct Borrowing

```rust
struct User {
    name: String,
    age: u32,
}

fn print_user(user: &User) {
    println!("Name: {}, Age: {}", user.name, user.age);
}

fn increment_age(user: &mut User) {
    user.age += 1;
}

fn main() {
    let mut user = User {
        name: String::from("Alice"),
        age: 30,
    };
    
    print_user(&user);        // Immutable borrow
    increment_age(&mut user); // Mutable borrow
    print_user(&user);        // Immutable borrow again
}
```

---

## Best Practices

### 1. **Prefer Borrowing Over Moving**

```rust
// ❌ Unnecessary move
fn expensive_function(s: String) {
    println!("{}", s);
}

// ✅ Borrow instead
fn better_function(s: &String) {
    println!("{}", s);
}
```

### 2. **Use References in Function Signatures**

```rust
// Clear that the function doesn't own the data
fn process(data: &mut Vec<i32>) {
    data.push(42);
}
```

### 3. **Minimize Mutable Borrow Duration**

```rust
fn main() {
    let mut s = String::from("hello");
    
    // ❌ Mutable borrow held longer than needed
    let r = &mut s;
    r.push_str(" world");
    println!("{}", r);
    
    // ❌ Can't do this because r is still in scope
    println!("{}", s);
    
    // ✅ Better: explicitly drop the reference
    drop(r);
    println!("{}", s);
}
```

### 4. **Understand Lifetime in Function Signatures**

```rust
// For &str (string slice), lifetime is usually implicit
fn get_first_word(s: &str) -> &str {
    // Returns a reference that lives as long as s
    &s[..1]
}

// More explicit version with lifetime annotations
fn get_first_word_explicit<'a>(s: &'a str) -> &'a str {
    &s[..1]
}

fn main() {
    let sentence = String::from("Hello World");
    let word = get_first_word(&sentence);
    println!("{}", word);  // "H"
}
```

### 5. **Remember: The Compiler is Your Friend**

When you get a borrowing error, read the error message carefully. Rust's compiler provides helpful suggestions:

```
error[E0502]: cannot borrow `s` as mutable because it is also borrowed as immutable
  --> src/main.rs:5:13
   |
3  |     let r1 = &s;
   |              -- immutable borrow occurs here
4  |     let r2 = &s;
5  |     let r3 = &mut s;
   |              ^^^^^^ mutable borrow occurs here
6  |
7  |     println!("{}, {}, {}", r1, r2, r3);
   |                            -- immutable borrow later used here
```

The compiler tells you exactly where the conflict is and which borrows are involved.

---

## Summary Table

| Type | Notation | Count | Mutability | Use Case |
|------|----------|-------|------------|----------|
| Ownership | `T` | 1 | Yes | Own the data |
| Immutable Borrow | `&T` | Many | No | Read data without owning |
| Mutable Borrow | `&mut T` | 1 | Yes | Modify data without owning |

---

## Key Takeaways

✅ **Ownership**: Each value has exactly one owner  
✅ **Borrowing**: References let you use data without owning it  
✅ **Immutable References**: Multiple readers, no writers  
✅ **Mutable References**: Exclusive access for one writer  
✅ **Compiler Enforces**: All rules checked at compile time, zero runtime cost  
✅ **No Dangling References**: Impossible in safe Rust  
✅ **Memory Safe**: No segfaults, no use-after-free, no data races (in safe code)  

Understanding these rules is the foundation of becoming proficient in Rust. The compiler might seem strict at first, but it's preventing entire categories of bugs before your code even runs!

