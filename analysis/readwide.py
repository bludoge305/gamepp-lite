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
    p=spec.split(':')
    va=int(p[0],16); n=int(p[1]) if len(p)>1 else None
    o=v2o(va)
    if n is None:
        s=b''
        while data[o:o+2]!=b'\x00\x00': s+=data[o:o+2]; o+=2
        print(s.decode('utf-16le'))
    else:
        print(data[v2o(va):v2o(va)+n*2].decode('utf-16le'))
