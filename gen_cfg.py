import sys

def generate_count_pop_cfg(rule_str: str) -> str:
    b_req = [False] * 9
    s_req = [False] * 9

    rule_upper = rule_str.upper()
    
    if not rule_upper.startswith('B'):
        raise ValueError(f"Strict format violation: Rule must start with 'B' (e.g., 'B3S23'). Found: {rule_str}")

    s_idx = rule_upper.find('S')
    if s_idx == -1:
        raise ValueError(f"Strict format violation: Rule must contain exactly one 'S' separating birth and survival conditions. Found: {rule_str}")

    b_str = rule_upper[1:s_idx]
    s_str = rule_upper[s_idx+1:]

    if 'S' in s_str or 'B' in s_str:
         raise ValueError(f"Strict format violation: Multiple 'B' or 'S' characters found. Found: {rule_str}")

    def parse_part(part_str: str, req: list[bool], part_name: str):
        for c in part_str:
            if not c.isdigit():
                raise ValueError(f"Strict format violation: Invalid character '{c}' in {part_name} section. Only digits 0-8 are allowed.")
            
            digit = int(c)
            
            if digit > 8:
                raise ValueError(f"Strict format violation: Digit '{digit}' out of bounds in {part_name} section. Moore neighborhood max is 8.")
            
            if req[digit]:
                raise ValueError(f"Strict format violation: Duplicate digit '{digit}' in {part_name} section.")
            
            req[digit] = True

    parse_part(b_str, b_req, "Birth")
    parse_part(s_str, s_req, "Survival")

    cfg_lines = []

    for gen0_val in range(512):
        # Extract bits based on the packed structure (NW=8 .. SE=0)
        se = (gen0_val >> 0) & 1
        s  = (gen0_val >> 1) & 1
        sw = (gen0_val >> 2) & 1
        e  = (gen0_val >> 3) & 1
        c  = (gen0_val >> 4) & 1
        w  = (gen0_val >> 5) & 1
        ne = (gen0_val >> 6) & 1
        n  = (gen0_val >> 7) & 1
        nw = (gen0_val >> 8) & 1

        neighbors = n + ne + e + se + s + sw + w + nw

        if c == 1:
            c_prime = 1 if s_req[neighbors] else 0
        else:
            c_prime = 1 if b_req[neighbors] else 0

        # For population count, score is 1 if it becomes alive, else 0
        score = c

        # Output in Golly order: C, N, NE, E, SE, S, SW, W, NW, C' = score
        cfg_lines.append(f"{c},{n},{ne},{e},{se},{s},{sw},{w},{nw},{c_prime}={score}")

    return "\n".join(cfg_lines) + "\n"

if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("Usage: python generate_cfg.py <rule_string>")
        print("Example: python generate_cfg.py b3s23")
        sys.exit(1)
    
    rule = sys.argv[1]
    
    try:
        config_output = generate_count_pop_cfg(rule)
        print(config_output, end="")
    except ValueError as e:
        print(e, file=sys.stderr)
        sys.exit(1)