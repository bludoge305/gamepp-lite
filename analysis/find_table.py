import struct
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
    for v,vs,ra,rs in secs:
        if v<=va<v+max(vs,rs): return ra+(va-v)
    return None
def is_wstr(va):
    o=v2o(va-0x10000000)
    if o is None: return False
    n=0
    while n<80:
        b=data[o+2*n:o+2*n+2]
        if len(b)<2: return False
        c=struct.unpack('<H',b)[0]
        if c==0: return n>=2
        if not (32<=c<127): return False
        n+=1
    return False
# scan .rdata for pointer runs
rv, rvs, rr, rrs = secs[1]
start=v2o(rv)
end=start+rrs
i=start
runs=[]
cur=[]
while i+4<=end:
    val=struct.unpack_from('<I',data,i)[0]
    if 0x10000000+rv <= val < 0x10000000+rv+rvs and is_wstr(val):
        cur.append((i,val))
    else:
        if len(cur)>=10: runs.append(cur)
        cur=[]
    i+=4
if len(cur)>=10: runs.append(cur)
print('runs found:',len(runs))
for r in runs[:5]:
    print('count',len(r),'at file off',hex(r[0][0]))
    for off,va in r:
        o=v2o(va-0x10000000)
        s=data[o:o+120].decode('utf-16le','ignore').split('\x00')[0]
        print('   ',s)
