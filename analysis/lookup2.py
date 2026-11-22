import sys,bisect
keys=[];vals={}
for line in open('va_strings.txt',encoding='utf-8'):
    p=line.rstrip('\n').split('\t')
    if len(p)<3: continue
    v=int(p[0],16)
    keys.append(v); vals.setdefault(v,(p[2],p[1]))
keys=sorted(set(keys))
for a in sys.argv[1:]:
    v=int(a,16)-0x10000000
    i=bisect.bisect_right(keys,v)-1
    if i>=0 and v-keys[i]<=0:
        print('%-10s -> %s'%(a,vals[keys[i]]))
    elif i>=0:
        k=keys[i]
        print('%-10s -> [%+d] %s'%(a,v-k,vals[k]))
    else:
        print('%-10s -> N/A'%a)
