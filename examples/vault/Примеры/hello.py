"""An ordinary source file beside the notes. Opening it does not execute it."""


def greet(name: str) -> str:
    return f"Hello, {name}!"


if __name__ == "__main__":
    print(greet("Diamond"))
