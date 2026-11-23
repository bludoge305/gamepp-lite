import struct,sys
import os
path=os.path.join(os.environ.get("GAMEPP_DIR", os.path.expandvars(r"%LOCALAPPDATA%\GamePP\Apps\8fb708e5972662062da26a2bb4753a23\5.6.99.701")), r"win32\common\GamePPOptimize.node")
data=open(path,'rb').read()
e=struct.unpack_from('<I',data,0x3C)[0];coff=e+4
numsec,=struct.unpack_from('<H',data,coff+2);opt_size,=struct.unpack_from('<H',data,coff+16)
opt=coff+20;sec_off=opt+opt_size
secs=[]
for i in range(numsec):
    o=sec_off+i*40
    vsize,vaddr,rsize,raddr=struct.unpack_from('<IIII',data,o+8)
    secs.append((vaddr,vsize,raddr,rsize))
def v2o(va):
    rva=va-0x10000000
    for v,vs,ra,rs in secs:
        if v<=rva<v+max(vs,rs): return ra+(rva-v)
    return None
for spec in sys.argv[1:]:
    a,n=(spec.split(':')+[64])[:2]
    o=v2o(int(a,16)); chunk=data[o:o+int(n)]
    print('== %s'%a)
    for i in range(0,len(chunk),16):
        row=chunk[i:i+16]
        hexs=' '.join('%02x'%b for b in row)
        asc=''.join(chr(b) if 32<=b<127 else '.' for b in row)
        u16=row.decode('utf-16le','replace')
        u16=''.join(c if 32<=ord(c)<127 else '.' for c in u16)
        print('%08x  %-47s  %-16s %s'%(int(a,16)+i,hexs,asc,u16))
