import hashlib,json,os,pathlib,sqlite3,subprocess,time,urllib.request,urllib.error,urllib.parse
root=pathlib.Path(os.environ['PALLY_TEST_DIR']).resolve()
root.mkdir(exist_ok=True)
db=root/'isolated.db'
assert not db.exists(), 'Use a fresh test database'
env=os.environ.copy()
for k in list(env):
    if k.startswith(('PALLY_','FK_')): env.pop(k)
env.update(CURSED_DB=str(db),CURSED_LISTEN='127.0.0.1:18086',CURSED_ORIGIN='http://127.0.0.1:18086',CURSED_SECURE_COOKIE='0',CURSED_MEDIA=str(root/'media'),CURSED_DIST=str(root/'dist'),PALLY_API_TOKEN='local-test-token',PALLY_SHOP_ID='local-test-shop')
log=open(root/'server.log','wb')
proc=subprocess.Popen([os.environ['PALLY_TEST_BINARY'],'serve'],env=env,stdout=log,stderr=log)
class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self,*a): return None
client=urllib.request.build_opener(NoRedirect)
def request(path,data=None,headers=None):
    req=urllib.request.Request('http://127.0.0.1:18086'+path,data=data,headers=headers or {})
    try: r=client.open(req,timeout=4)
    except urllib.error.HTTPError as e:r=e
    return r.status,r.headers,r.read()
try:
    for _ in range(60):
        try:
            if request('/api/pay/pally')[0]==200:break
        except OSError: pass
        if proc.poll() is not None: raise RuntimeError('server failed; inspect log')
        time.sleep(.25)
    else:raise RuntimeError('startup timeout')
    for suffix in ['ok','fail']:
        for body in [None,b'InvId=rcd-test']:
            code,h,b=request('/api/pay/'+suffix,body,{'Content-Type':'application/x-www-form-urlencoded'})
            assert code==303,(suffix,code,b)
            code,h,b=request(h['Location']);assert code==200
            assert 'Р›РёС‡РЅС‹Р№ РєР°Р±РёРЅРµС‚'.encode() in b
            (root/(suffix+'.html')).write_bytes(b)
    assert request('/api/pay/result.js')[0]==200
    assert request('/api/pay/pally/status?id=unknown')[0]==401
    headers={'Content-Type':'application/json','Origin':'http://127.0.0.1:18086'}
    assert request('/api/pay/pally/create',b'{"days":30,"agree":true}',headers)[0]==401
    assert request('/api/pay/pally',b'InvId=fake&OutSum=299&SignatureValue=bad')[0]==403
    assert request('/api/pay/pally',b'x'*9000)[0]==413
    at=int(time.time());token='a'*64;session=hashlib.sha256(token.encode()).digest()
    with sqlite3.connect(db) as c:
        c.execute("INSERT INTO users(uid,login,login_key,email,email_key,pass,created) VALUES(42,'testbuyer','testbuyer','test@example.invalid','test@example.invalid','unused',?)",(at,))
        c.execute("INSERT INTO sessions(hash,uid,kind,created,seen) VALUES(?,42,'web',?,?)",(session,at,at))
        c.execute("INSERT INTO pally_orders(id,uid,days,amount,bill,made,fulfilled) VALUES('rcd-owned',42,30,29900,'bill42',?,?)",(at,at))
        c.execute("INSERT INTO pally_orders(id,uid,days,amount,bill,made,fulfilled) VALUES('rcd-other',43,30,29900,'bill43',?,?)",(at,at))
    headers['Cookie']='cursed_session='+token
    assert request('/api/pay/pally/create',b'{"days":1,"agree":true}',headers)[0]==400
    assert request('/api/pay/pally/create',b'{"days":30,"agree":false}',headers)[0]==400
    assert request('/api/pay/pally/create',b'{"days":30,"agree":true,"method":"crypto"}',headers)[0]==400
    bad=headers.copy();bad['Origin']='https://other.invalid'
    assert request('/api/pay/pally/create',b'{"days":30,"agree":true}',bad)[0]==403
    assert request('/api/pay/pally/status?id=rcd-other',headers=headers)[0]==404
    code,h,b=request('/api/pay/pally/status?id=rcd-owned',headers=headers)
    assert code==200 and json.loads(b)['paid'] is True
    fields={'InvId':'rcd-unknown','OutSum':'299.00','TrsId':'bill-unknown','CurrencyIn':'RUB','Status':'SUCCESS'}
    fields['SignatureValue']=hashlib.md5(b'299.00:rcd-unknown:local-test-token').hexdigest().upper()
    assert request('/api/pay/pally',urllib.parse.urlencode(fields).encode())[0]==400
    with sqlite3.connect(db) as c:
        assert c.execute('SELECT count(*) FROM payments').fetchone()[0]==0
        assert c.execute('SELECT count(*) FROM recode_keys').fetchone()[0]==0
    print('PASS: GET/POST returns, result page/script, auth, ownership, consent, prices, origin, invalid signatures, unknown signed orders, body limits; no subscription/payment changes')
finally:
    proc.terminate()
    try:proc.wait(timeout=10)
    except subprocess.TimeoutExpired:proc.kill();proc.wait()
    log.close()
