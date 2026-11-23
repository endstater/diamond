# Markdown 0.6

- Parent item
  - Child item
    - [ ] Nested task
    - [x] Completed nested task
  1. Numbered inside bullets
  2. Next item
- Next parent

1. First
   1. Nested one
   2. Nested two
2. Second

> First paragraph, **bold** and `code`.
>
> Second paragraph.
>> Nested quote
>>> Third level

Formula in a line: $e^{i\pi}+1=0$.

$$
\frac{1}{2}+\sqrt{x^2+y^2}
$$

A footnote[^note] and the same footnote again[^note].

Before the code block:
```rust
fn main() {
    println!("hello");
}
```

```
No language specified
```

[^note]: A **formatted** definition.

    A second paragraph with $x^2$.
