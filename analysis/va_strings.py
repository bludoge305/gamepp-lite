import re,struct,sys
import os
path=os.path.join(os.environ.get("GAMEPP_DIR", os.path.expandvars(r"%LOCALAPPDATA%\GamePP\Apps\8fb708e5972662062da26a2bb4753a23\5.6.99.701")), r"win32\common\GamePPOptimize.node")
data=open(path,'rb').read()
e=struct.unpack_from('<I',data,0x3C)[0]
coff=e+4; numsec,=struct.unpack_from('<H',data,coff+2); opt_size,=struct.unpack_from('<H',data,coff+16)
opt=coff+20; sec_off=opt+opt_size
secs=[]
for i in range(numsec):
    o=sec_off+i*40
    vsize,vaddr,rsize,raddr=struct.unpack_from('<IIII',data,o+8)
    secs.append((vaddr,vsize,raddr,rsize))
def o2v(off):
    for va,vs,ra,rs in secs:
        if ra<=off<ra+rs: return va+(off-ra)
    return None
rows=[]
for m in re.finditer(rb'[\x20-\x7e]{4,}', data):
    v=o2v(m.start())
    if v: rows.append((v,'A',m.group().decode('latin1')))
for m in re.finditer(rb'(?:[\x20-\x7e]\x00){4,}', data):
    v=o2v(m.start())
    if v: rows.append((v,'U',m.group().decode('utf-16le')))
rows.sort()
with open('va_strings.txt','w',encoding='utf-8') as f:
    for v,k,s in rows: f.write('%08x\t%s\t%s\n'%(v,k,s))
print(len(rows))
