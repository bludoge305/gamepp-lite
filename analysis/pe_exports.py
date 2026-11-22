import struct,sys
p=sys.argv[1]
d=open(p,'rb').read()
e=struct.unpack_from('<I',d,0x3C)[0]
coff=e+4;nsec,=struct.unpack_from('<H',d,coff+2);osz,=struct.unpack_from('<H',d,coff+16)
opt=coff+20;magic,=struct.unpack_from('<H',d,opt)
is64=magic==0x20b
dd=opt+(112 if is64 else 96)
so=opt+osz
secs=[]
for i in range(nsec):
    o=so+i*40
    vs,va,rs,ra=struct.unpack_from('<IIII',d,o+8)
    secs.append((va,vs,ra,rs))
def r2o(r):
    for va,vs,ra,rs in secs:
        if va<=r<va+max(vs,rs): return ra+(r-va)
    return None
def cstr(o):
    return d[o:d.index(b'\0',o)].decode('latin1')
erva,esz=struct.unpack_from('<II',d,dd)
if erva==0:
    print('no exports');sys.exit()
o=r2o(erva)
name,nbase,nfunc,nname,addrfunc,addrname,addrord=struct.unpack_from('<IIIIIII',d,o)
print('dll',cstr(r2o(name)),'nfunc',nfunc,'nnames',nname)
names=[cstr(r2o(struct.unpack_from('<I',d,r2o(addrname)+4*i)[0])) for i in range(nname)]
ordinals=[struct.unpack_from('<H',d,r2o(addrord)+2*i)[0] for i in range(nname)]
for nm,od in sorted(zip(names,ordinals)):
    print('  %-40s ord=%d' % (nm, od))
