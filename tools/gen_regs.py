RC0_OFFSET = 16
RS0_OFFSET = 528

offset = 0
regs = []

regs.append(f'<reg name="PC" bitsize="16" offset="{offset}" regnum="0" generic="pc" />')
offset += 2

regs.append(f'<reg name="A" bitsize="8" offset="{offset}" regnum="1" dwarf_regnum="0" />')
offset += 1

regs.append(f'<reg name="X" bitsize="8" offset="{offset}" regnum="2" dwarf_regnum="2" />')
offset += 1

regs.append(f'<reg name="Y" bitsize="8" offset="{offset}" regnum="3" dwarf_regnum="4" />')
offset += 1

regs.append(f'<reg name="S" bitsize="8" offset="{offset}" regnum="4" />')
offset += 1

regs.append(f'<reg name="C" bitsize="1" offset="{offset}" regnum="5" />')
offset += 1

regs.append(f'<reg name="Z" bitsize="1" offset="{offset}" regnum="6" />')
offset += 1

regs.append(f'<reg name="V" bitsize="1" offset="{offset}" regnum="7" />')
offset += 1

regs.append(f'<reg name="N" bitsize="1" offset="{offset}" regnum="8" />')
offset += 1

regs.extend(f'<reg name="RC{i}" group_id="1" bitsize="8" offset="{offset + i}" regnum="{9 + i}" dwarf_regnum="{RC0_OFFSET + i * 2}" />' for i in range(32))
offset += 32

regs.extend(f'<reg name="RS{i}" group_id="2" bitsize="16" offset="{offset + i * 2}" regnum="{9 + i + 32}" dwarf_regnum="{RS0_OFFSET + i}" />' for i in range(16))
offset += 32

print("\n".join(regs))
