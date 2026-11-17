"""Inspect a Windows PE file without executing it or using third-party packages."""
from pathlib import Path
import hashlib
import json
import struct
import sys


def inspect(path):
    data = Path(path).read_bytes()
    if data[:2] != b"MZ":
        raise ValueError("Not a Windows executable")
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        raise ValueError("Invalid PE header")
    machine, sections = struct.unpack_from("<HH", data, pe + 4)
    optional_size = struct.unpack_from("<H", data, pe + 20)[0]
    optional = pe + 24
    if machine != 0x8664 or struct.unpack_from("<H", data, optional)[0] != 0x20B:
        raise ValueError("Expected a Windows x64 PE32+ executable")
    subsystem = struct.unpack_from("<H", data, optional + 68)[0]
    headers = struct.unpack_from("<I", data, optional + 60)[0]
    table = optional + optional_size

    def offset(rva):
        if rva < headers:
            return rva
        for index in range(sections):
            section = table + index * 40
            virtual_size, address, raw_size, raw_pointer = struct.unpack_from("<IIII", data, section + 8)
            if address <= rva < address + max(virtual_size, raw_size):
                return raw_pointer + rva - address
        raise ValueError("Import RVA is not mapped")

    imports = []
    import_rva = struct.unpack_from("<I", data, optional + 120)[0]
    if import_rva:
        position = offset(import_rva)
        while any(data[position:position + 20]):
            name = offset(struct.unpack_from("<I", data, position + 12)[0])
            end = data.index(b"\0", name)
            imports.append(data[name:end].decode("ascii").lower())
            position += 20
    forbidden = [name for name in imports if name.startswith(("vcruntime", "msvcp", "api-ms-win-crt"))
                 or name == "ucrtbase.dll"]
    embedded = all(text in data for text in [b"SIL OPEN FONT LICENSE Version 1.1", b"MIT License", b"CC0 1.0"])
    report = {"file": Path(path).name, "machine": "x64", "windows_gui": subsystem == 2,
              "imports": imports, "dynamic_crt_imports": forbidden,
              "embedded_license_notices": embedded, "bytes": len(data),
              "sha256": hashlib.sha256(data).hexdigest()}
    if subsystem != 2 or forbidden or not embedded:
        raise ValueError("Portable executable validation failed: " + json.dumps(report))
    return report


if __name__ == "__main__":
    print(json.dumps(inspect(sys.argv[1]), indent=2))
