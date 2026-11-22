import struct, sys

def main(path):
    data = open(path,'rb').read()
    e_lfanew = struct.unpack_from('<I', data, 0x3C)[0]
    assert data[e_lfanew:e_lfanew+4] == b'PE\0\0'
    coff = e_lfanew+4
    numsec, = struct.unpack_from('<H', data, coff+2)
    opt_size, = struct.unpack_from('<H', data, coff+16)
    opt = coff+20
    magic, = struct.unpack_from('<H', data, opt)
    is64 = magic == 0x20b
    ddoff = opt + (112 if is64 else 96)
    sec_off = opt + opt_size
    secs=[]
    for i in range(numsec):
        o = sec_off + i*40
        name = data[o:o+8].rstrip(b'\0').decode('latin1')
        vsize, vaddr, rsize, raddr = struct.unpack_from('<IIII', data, o+8)
        secs.append((name,vaddr,vsize,raddr,rsize))
    def rva2off(rva):
        for name,va,vs,ra,rs in secs:
            if va <= rva < va+max(vs,rs):
                return ra + (rva-va)
        return None
    def cstr(off):
        e = data.index(b'\0', off)
        return data[off:e].decode('latin1')
    imp_rva, = struct.unpack_from('<I', data, ddoff)
    imp_sz, = struct.unpack_from('<I', data, ddoff+4)
    off = rva2off(imp_rva)
    out=[]
    while True:
        ilt, ts, fwd, nameRVA, iat = struct.unpack_from('<IIIII', data, off)
        if nameRVA == 0 and ilt == 0 and iat == 0:
            break
        dll = cstr(rva2off(nameRVA))
        thunk = ilt or iat
        toff = rva2off(thunk)
        funcs=[]
        while True:
            val, = struct.unpack_from('<I', data, toff)
            if val == 0: break
            if val & 0x80000000:
                funcs.append('#%d' % (val & 0xffff))
            else:
                funcs.append(cstr(rva2off(val)+2))
            toff += 4
        out.append((dll, funcs))
        off += 20
    for dll, funcs in out:
        print('== %s (%d)' % (dll, len(funcs)))
        for f in sorted(funcs):
            print('   ', f)
    print('TOTAL', sum(len(f) for _,f in out))

main(sys.argv[1])
