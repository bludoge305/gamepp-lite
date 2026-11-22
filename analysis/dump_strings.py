import re,sys
path=sys.argv[1]
data=open(path,'rb').read()
for m in re.finditer(rb'[\x20-\x7e]{4,}', data):
    print('%08x\t%s' % (m.start(), m.group().decode('latin1')))
for m in re.finditer(rb'(?:[\x20-\x7e]\x00){4,}', data):
    print('%08x\tU\t%s' % (m.start(), m.group().decode('utf-16le')))
