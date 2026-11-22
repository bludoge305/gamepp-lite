import sys
rows={}
for line in open('va_strings.txt',encoding='utf-8'):
    p=line.rstrip('\n').split('\t')
    if len(p)<3: continue
    try: v=int(p[0],16)
    except: continue
    rows.setdefault(v,[]).append((p[2],p[1])) if False else rows.__setitem__(v,(p[2],p[1]))
for a in sys.argv[1:]:
    v=int(a,16)-0x10000000
    hit=None
    for k in sorted(rows):
        if k<=v<k+400:
            hit=(k,rows[k]); break
    print('%-10s -> %s' % (a, hit[1] if hit else 'N/A'))
