#!/usr/bin/env python3
"""Read-only LuaJIT v1 dump inspection; never loads or executes target code.

Schema reference: LuaJIT v2.0 src/lj_bcdump.h, lj_bcread.c, lj_bc.h.
This is an independently written parser, not a Lua interpreter/decompiler.
Offsets are absolute file byte offsets; PCs start at 1 (implicit header omitted).
GC indices are instruction operands, i.e. reverse of serialized constant order.
Table arrays include Lua key 0, even when that slot is nil. Hash pairs remain
pairs to preserve numeric/boolean keys without JSON object-key coercion.
"""

import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import sys
import unittest


OPCODES = """ISLT ISGE ISLE ISGT ISEQV ISNEV ISEQS ISNES ISEQN ISNEN ISEQP ISNEP
ISTC ISFC IST ISF MOV NOT UNM LEN ADDVN SUBVN MULVN DIVVN MODVN ADDNV SUBNV
MULNV DIVNV MODNV ADDVV SUBVV MULVV DIVVV MODVV POW CAT KSTR KCDATA KSHORT
KNUM KPRI KNIL UGET USETV USETS USETN USETP UCLO FNEW TNEW TDUP GGET GSET
TGETV TGETS TGETB TSETV TSETS TSETB TSETM CALLM CALL CALLMT CALLT ITERC ITERN
VARG ISNEXT RETM RET RET0 RET1 FORI JFORI FORL IFORL JFORL ITERL IITERL
JITERL LOOP ILOOP JLOOP JMP FUNCF IFUNCF JFUNCF FUNCV IFUNCV JFUNCV FUNCC
FUNCCW""".split()
ABC = set("""ADDVN SUBVN MULVN DIVVN MODVN ADDNV SUBNV MULNV DIVNV MODNV
ADDVV SUBVV MULVV DIVVV MODVV POW CAT TGETV TGETS TGETB TSETV TSETS TSETB
CALLM CALL ITERC ITERN VARG""".split())
GC_OPS = set("ISEQS ISNES KSTR KCDATA USETS FNEW TDUP GGET GSET TGETS TSETS".split())
NUM_OPS = set("ISEQN ISNEN KNUM USETN TSETM ADDVN SUBVN MULVN DIVVN MODVN ADDNV SUBNV MULNV DIVNV MODNV".split())
JUMPS = set("UCLO ISNEXT FORI JFORI FORL IFORL ITERL IITERL LOOP ILOOP JMP".split())


class FormatError(ValueError):
    pass


def signed(value, bits):
    return value - (1 << bits) if value & (1 << (bits - 1)) else value


def double(lo, hi):
    value = struct.unpack("<d", struct.pack("<II", lo, hi))[0]
    # Standard JSON cannot represent infinities/NaNs; retain the bits separately.
    return value if math.isfinite(value) else str(value)


class Reader:
    def __init__(self, data, start=0, end=None):
        self.data, self.pos = data, start
        self.end = len(data) if end is None else end

    def take(self, size):
        if size < 0 or size > self.end - self.pos:
            raise FormatError(f"truncated data at 0x{self.pos:x}: need {size} bytes")
        result = self.data[self.pos:self.pos + size]
        self.pos += size
        return result

    def byte(self):
        return self.take(1)[0]

    def uleb(self, bits=32):
        value = 0
        for shift in range(0, bits, 7):
            byte = self.byte()
            value |= (byte & 127) << shift
            if not byte & 128:
                if value >= 1 << bits:
                    break
                return value
        raise FormatError(f"ULEB{bits} overflow at 0x{self.pos:x}")

    def text(self, size):
        return self.take(size).decode("utf-8", errors="surrogateescape")

    def table_value(self):
        offset = self.pos
        tag = self.uleb()
        item = {"offset": offset, "tag": tag}
        if tag >= 5:
            item.update(type="string", value=self.text(tag - 5))
        elif tag == 3:
            item.update(type="integer", value=signed(self.uleb(), 32))
        elif tag == 4:
            lo, hi = self.uleb(), self.uleb()
            item.update(type="number", value=double(lo, hi), lo=lo, hi=hi)
        else:
            item.update(type=("nil", "boolean", "boolean")[tag],
                        value=(None, False, True)[tag])
        item["end_offset"] = self.pos
        return item

    def gc(self, stack):
        offset = self.pos
        tag = self.uleb()
        item = {"offset": offset, "tag": tag}
        if tag >= 5:
            item.update(type="string", value=self.text(tag - 5))
        elif tag == 0:
            if not stack:
                raise FormatError(f"child prototype stack underflow at 0x{offset:x}")
            item.update(type="child", prototype=stack.pop())
        elif tag == 1:
            na, nh = self.uleb(), self.uleb()
            if na + 2 * nh > self.end - self.pos:
                raise FormatError(f"impossible table size at 0x{offset:x}")
            array = [self.table_value() for _ in range(na)]
            pairs = [[self.table_value(), self.table_value()] for _ in range(nh)]
            if any(key["value"] is None for key, _ in pairs):
                raise FormatError(f"nil table key at 0x{offset:x}")
            item.update(type="table", array_base=0, array=array, hash=pairs)
        else:
            lo, hi = self.uleb(), self.uleb()
            if tag == 4:
                ilo, ihi = self.uleb(), self.uleb()
                item.update(type="complex", real=double(lo, hi), imag=double(ilo, ihi),
                            words=[lo, hi, ilo, ihi])
            else:
                value = lo | hi << 32
                item.update(type="i64" if tag == 2 else "u64",
                            value=signed(value, 64) if tag == 2 else value,
                            words=[lo, hi])
        item["end_offset"] = self.pos
        return item

    def number(self):
        offset = self.pos
        encoded = self.uleb(33)
        lo = encoded >> 1
        item = {"offset": offset}
        if encoded & 1:
            hi = self.uleb()
            item.update(type="number", value=double(lo, hi), lo=lo, hi=hi)
        else:
            item.update(type="integer", value=signed(lo, 32))
        item["end_offset"] = self.pos
        return item


def instructions(words, offset, endian, kgc, numbers):
    result = []
    for index in range(len(words) // 4):
        word = int.from_bytes(words[index * 4:index * 4 + 4], endian)
        op, a, c, b = word & 255, word >> 8 & 255, word >> 16 & 255, word >> 24
        if op >= len(OPCODES):
            raise FormatError(f"unknown opcode {op} at 0x{offset + 4 * index:x}")
        name, d = OPCODES[op], word >> 16
        ins = dict(pc=index + 1, offset=offset + 4 * index, word=f"{word:08x}",
                   opcode=op, name=name, A=a)
        ins.update({"B": b, "C": c} if name in ABC else {"D": d})
        operand = c if name in ABC else d
        if name in GC_OPS:
            if operand >= len(kgc):
                raise FormatError(f"GC operand out of range at 0x{ins['offset']:x}")
            ins.update(gc_index=operand, constant=kgc[operand])
        if name in NUM_OPS:
            if operand >= len(numbers):
                raise FormatError(f"number operand out of range at 0x{ins['offset']:x}")
            ins.update(number_index=operand, constant=numbers[operand])
        if name == "KSHORT":
            ins["literal"] = signed(d, 16)
        if name in {"KPRI", "USETP", "ISEQP", "ISNEP"}:
            ins["literal"] = (None, False, True)[d] if d < 3 else {"invalid_primitive": d}
        if name in JUMPS:
            ins["target_pc"] = index + 2 + d - 0x8000
        result.append(ins)
    return result


def inspect(data, source="<bytes>"):
    r = Reader(data)
    if r.take(4) != b"\x1bLJ\x01":
        raise FormatError("expected LuaJIT v1 header (1b4c4a01)")
    flags = r.uleb()
    if flags & ~7:
        raise FormatError(f"unsupported header flags 0x{flags:x}")
    endian = "big" if flags & 1 else "little"
    chunk = None if flags & 2 else r.text(r.uleb())
    prototypes, stack = [], []
    while True:
        length_offset = r.pos
        length = r.uleb()
        if not length:
            break
        start = r.pos
        r.take(length)
        p = Reader(data, start, start + length)
        pf, params, frame, nuv = (p.byte() for _ in range(4))
        ngc, nn, nbc = p.uleb(), p.uleb(), p.uleb()
        debug_size = p.uleb() if not flags & 2 else 0
        firstline, numline = (p.uleb(), p.uleb()) if debug_size else (None, None)
        bc_offset = p.pos
        words = p.take(4 * nbc)
        uv_offset = p.pos
        uv = [int.from_bytes(p.take(2), endian) for _ in range(nuv)]
        if ngc + nn > p.end - p.pos:
            raise FormatError(f"impossible constant counts at 0x{start:x}")
        kgc = [p.gc(stack) for _ in range(ngc)][::-1]
        for i, item in enumerate(kgc):
            item["gc_index"] = i
        numbers = [p.number() for _ in range(nn)]
        debug_offset = p.pos
        p.take(debug_size)
        if p.pos != p.end:
            raise FormatError(f"prototype at 0x{start:x}: {p.end - p.pos} unparsed bytes")
        ident = len(prototypes)
        prototypes.append(dict(id=ident, length_offset=length_offset, offset=start,
                               end_offset=p.end, flags=pf, numparams=params,
                               framesize=frame, upvalues_offset=uv_offset, upvalues=uv,
                               gc=kgc, numbers=numbers, debug_offset=debug_offset,
                               debug_size=debug_size, firstline=firstline, numline=numline,
                               instructions=instructions(words, bc_offset, endian, kgc, numbers)))
        stack.append(ident)
    if r.pos != r.end:
        raise FormatError(f"trailing bytes at 0x{r.pos:x}")
    if len(stack) != 1:
        raise FormatError(f"expected one root prototype, found {len(stack)}")
    return dict(source=source, sha256=hashlib.sha256(data).hexdigest(), version=1,
                flags=flags, endian=endian, chunk=chunk, root=stack[0], prototypes=prototypes)


def listing(report):
    lines = [f"{report['source']} sha256={report['sha256']}"]
    for proto in report["prototypes"]:
        lines.append(f"prototype {proto['id']} @0x{proto['offset']:x} frame={proto['framesize']}")
        for ins in proto["instructions"]:
            operands = " ".join(f"{k}={ins[k]}" for k in ("A", "B", "C", "D") if k in ins)
            note = ""
            if "constant" in ins:
                note = " ; " + json.dumps(ins["constant"], ensure_ascii=True)
            elif "literal" in ins:
                note = " ; " + repr(ins["literal"])
            elif "target_pc" in ins:
                note = f" ; -> {ins['target_pc']}"
            lines.append(f"  {ins['pc']:04d} @0x{ins['offset']:06x} {ins['name']:<7} {operands}{note}")
    return "\n".join(lines)


def self_test():
    """Self-authored byte fixtures, no Lua compiler/runtime required."""
    def u(value):
        out = bytearray()
        while value >= 128:
            out.append((value & 127) | 128)
            value >>= 7
        return bytes(out + bytes([value]))

    class Tests(unittest.TestCase):
        def test_numbers(self):
            for value in (0.1, -0.0, 1.0, -123.75, 1e100):
                lo, hi = struct.unpack("<II", struct.pack("<d", value))
                number = Reader(u((lo << 1) | 1) + u(hi)).number()
                self.assertEqual(struct.pack("<d", number["value"]), struct.pack("<d", value))
            self.assertEqual(Reader(u(0xffffffff << 1)).number()["value"], -1)
            self.assertEqual(Reader(b"\x03" + u(0xffffffff)).table_value()["value"], -1)

        def test_dump(self):
            # Serialized string before table: TDUP D=0 must refer to the table.
            table = b"\x01\x03\x01\x00\x03\x0a\x02\x06x\x01"
            words = struct.pack("<II", OPCODES.index("TDUP"), OPCODES.index("RET1") | (2 << 16))
            body = b"\x00\x00\x01\x00\x02\x00\x02" + words + b"\x06s" + table
            data = b"\x1bLJ\x01\x02" + u(len(body)) + body + b"\x00"
            report = inspect(data)
            proto = report["prototypes"][0]
            t = proto["instructions"][0]["constant"]
            self.assertEqual(t["type"], "table")
            self.assertEqual([v["value"] for v in t["array"]], [None, 10, True])
            self.assertEqual([v["value"] for v in t["hash"][0]], ["x", False])
            self.assertEqual(proto["gc"][1]["value"], "s")
            for n in range(len(data)):
                with self.assertRaises(FormatError):
                    inspect(data[:n])
            with self.assertRaises(FormatError):
                inspect(data + b"x")

        def test_bad_uleb(self):
            with self.assertRaises(FormatError):
                Reader(b"\xff" * 5).uleb()

        def test_children_endian_debug(self):
            for endian, flag in (("little", 0), ("big", 1)):
                word = (OPCODES.index("RET0") | (1 << 16)).to_bytes(4, endian)
                # Unstripped child with one upvalue and one byte of debug data.
                child = (b"\x00\x00\x01\x01\x00\x00\x01\x01\x0a\x01"
                         + word + (0x8002).to_bytes(2, endian) + b"\x00")
                parent = b"\x01\x00\x01\x00\x01\x00\x01\x00" + word + b"\x00"
                data = (b"\x1bLJ\x01" + bytes([flag]) + b"\x04test"
                        + u(len(child)) + child + u(len(parent)) + parent + b"\x00")
                r = inspect(data)
                self.assertEqual(r["root"], 1)
                self.assertEqual(r["chunk"], "test")
                self.assertEqual(r["prototypes"][0]["upvalues"], [0x8002])
                self.assertEqual(r["prototypes"][1]["gc"][0]["prototype"], 0)
                self.assertEqual(r["prototypes"][0]["instructions"][0]["name"], "RET0")

        def test_cdata(self):
            self.assertEqual(Reader(b"\x02" + u(0xffffffff) * 2).gc([])["value"], -1)
            self.assertEqual(Reader(b"\x03" + u(0xffffffff) * 2).gc([])["value"], (1 << 64) - 1)
            lo, hi = struct.unpack("<II", struct.pack("<d", 2.5))
            c = Reader(b"\x04" + (u(lo) + u(hi)) * 2).gc([])
            self.assertEqual((c["real"], c["imag"]), (2.5, 2.5))

    result = unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(Tests))
    return 0 if result.wasSuccessful() else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("paths", nargs="*", type=Path)
    parser.add_argument("--json", action="store_true", help="emit constants, offsets and instructions as JSON")
    parser.add_argument("--self-test", action="store_true", help="run self-authored synthetic fixtures")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if not args.paths:
        parser.error("supply bytecode paths or --self-test")
    try:
        reports = [inspect(path.read_bytes(), str(path)) for path in args.paths]
    except (OSError, FormatError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(json.dumps(reports, indent=2, ensure_ascii=True, allow_nan=False) if args.json
          else "\n\n".join(listing(report) for report in reports))
    return 0


if __name__ == "__main__":
    sys.exit(main())
