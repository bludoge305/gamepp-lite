import json,sys,os
p=r'C:/Users/Bludoge/AppData/Local/GamePP/Apps/8fb708e5972662062da26a2bb4753a23/5.6.99.701/webapp.gpk'
raw=open(p,'rb').read()
txt=raw[16:].decode('utf-8','replace')
dec=json.JSONDecoder()
obj,end=dec.raw_decode(txt)
print('json chars',end,'entries')
out=sys.argv[1] if len(sys.argv)>1 else None
cnt=[0]
def walk(node,path):
    if 'files' in node:
        for k,v in node['files'].items(): walk(v,path+'/'+k)
    else:
        cnt[0]+=1
        size=node.get('size'); off=node.get('offset')
        if cnt[0]<=8: print(path, node)
        if out and path.endswith('.js') and off is not None:
            fp=os.path.join(out,path.strip('/').replace('/','_'))
            open(fp,'wb').write(raw[off:off+size])
walk(obj,'')
print('files',cnt[0])
