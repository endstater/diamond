# Rendering checks

Move the caret onto an empty line after these items. Previous markers should collapse immediately.

- [ ] First task
- [ ] Second task

- [Example](https://example.org "Title")
- [[Начало|Wiki link]]
- Autolink: https://example.org

1. One
2. Two

---

| Name | Count | Description |
| :--- | ---: | :---: |
| **Bold** | 42 | [Link](https://example.org) |
| 你好 | 7 | *Italic* |
| Empty | | cell |

```rust
// Rust
fn main() {
    let message = "你好 · Привет";
    println!("{message}");
}
```

```python
# Python
for value in range(3):
    print(f"value={value}")
```

![Alt text](https://example.org/sample.png "Remote image: click Load image; this example URL may return an error")
