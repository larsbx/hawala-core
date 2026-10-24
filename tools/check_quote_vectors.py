"""Independent arbitrary-precision oracle. Does NOT run/validate Rust code."""
from pathlib import Path

MAX = (1 << 64) - 1


def calculate(p, sf, df, num, den):
    if not p or not num or not den:
        return "Invalid"
    if p + sf > MAX:
        return "Overflow"
    gross, remainder = divmod(p * num, den)
    if gross > MAX:
        return "Overflow"
    if gross <= df:
        return "Invalid"
    return f"{p + sf},{gross},{gross - df},{remainder}"


def main():
    path = Path(__file__).resolve().parents[1] / "spec/quote-vectors.tsv"
    count = 0
    for line in path.read_text().splitlines():
        if not line or line.startswith("#"):
            continue
        name, *fields = line.split("\t")
        expected = fields.pop()
        actual = calculate(*map(int, fields))
        if actual != expected:
            raise AssertionError((name, expected, actual))
        count += 1
    print(f"PASS: {count} independent quote vectors (not Rust execution)")


if __name__ == "__main__":
    main()
