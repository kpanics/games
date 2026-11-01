import hashlib,hmac,json,os,pathlib,sqlite3,subprocess,time,urllib.request,urllib.error,tempfile
root=pathlib.Path(tempfile.mkdtemp(prefix='rolly-smoke-',dir=os.environ['ROLLYPAY_TEST_DIR']))
db=root/'isolated.db';at=int(time.time());secret='local-test-secret'
env=os.environ.copy()
env['ROLLYPAY_ENABLED']='1'
env.update(CURSED_DB=str(db),CURSED_LISTEN='127.0.0.1:18086',CURSED_ORIGIN='http://127.0.0.1:18086',CURSED_SECURE_COOKIE='0',CURSED_MEDIA=str(root/'media'),CURSED_DIST=str(root/'dist'),ROLLYPAY_API_KEY='unused-test',ROLLYPAY_TERMINAL_ID='unused-test',ROLLYPAY_SIGNING_SECRET=secret)
log=open(root/'server.log','wb')
proc=subprocess.Popen([os.environ['ROLLYPAY_TEST_BINARY'],'serve'],env=env,stdout=log,stderr=log)
class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self,*a):return None
client=urllib.request.build_opener(NoRedirect)
def request(path,data=None,headers=None):
    req=urllib.request.Request('http://127.0.0.1:18086'+path,data=data,headers=headers or {})
    try:r=client.open(req,timeout=5)
    except urllib.error.HTTPError as e:r=e
    return r.status,r.headers,r.read()
def event(state='paid',**extra):
    d=dict(event_type='payment.'+state,payment_id='bill42',order_id='rcd-owned',status=state,amount='299.00',currency='RUB',test=False);d.update(extra)
    data=json.dumps(d).encode();stamp=str(int(time.time()))
    sig=hmac.new(secret.encode(),stamp.encode()+b'.'+data,hashlib.sha256).hexdigest()
    return request('/api/pay/rollypay/callback',data,{'Content-Type':'application/json','X-Timestamp':stamp,'X-Signature':sig})
try:
    for _ in range(60):
        try:
            if request('/api/pay/rollypay/callback')[0]==200:break
        except OSError:pass
        if proc.poll() is not None:raise RuntimeError('startup failed '+str(root))
        time.sleep(.25)
    else:raise RuntimeError('startup timeout')
    for suffix in ['ok','fail']:
        for data in [None,b'id=rcd-test']:
            code,h,b=request('/api/pay/'+suffix,data,{'Content-Type':'application/x-www-form-urlencoded'})
            assert code==303,(code,b)
            if data:assert 'id=rcd-test' in h['Location']
            assert request(h['Location'])[0]==200
    assert request('/api/pay/result.js')[0]==200
    assert request('/api/pay/rollypay/status?id=unknown')[0]==401
    headers={'Content-Type':'application/json','Origin':'http://127.0.0.1:18086'}
    assert request('/api/pay/rollypay/create',b'{"days":30,"agree":true}',headers)[0]==401
    assert request('/api/pay/rollypay/callback',b'{}')[0]==403
    assert request('/api/pay/rollypay/callback',b'x'*9000)[0]==413
    token='a'*64;session=hashlib.sha256(token.encode()).digest()
    with sqlite3.connect(db) as c:
        c.execute("INSERT INTO users(uid,login,login_key,email,email_key,pass,created) VALUES(42,'testbuyer','testbuyer','test@example.invalid','test@example.invalid','unused',?)",(at,))
        c.execute("INSERT INTO sessions(hash,uid,kind,created,seen) VALUES(?,42,'web',?,?)",(session,at,at))
        c.execute("INSERT INTO rollypay_orders(id,uid,days,amount,bill,made) VALUES('rcd-owned',42,30,29900,'bill42',?)",(at,))
        c.execute("INSERT INTO rollypay_orders(id,uid,days,amount,bill,made) VALUES('rcd-other',43,30,29900,'bill43',?)",(at,))
    headers['Cookie']='cursed_session='+token
    for d in [{'days':1,'agree':True},{'days':30,'agree':False},{'days':30,'agree':True,'method':'card'}]:
        assert request('/api/pay/rollypay/create',json.dumps(d).encode(),headers)[0]==400
    bad=headers.copy();bad['Origin']='https://other.invalid'
    assert request('/api/pay/rollypay/create',b'{"days":30,"agree":true}',bad)[0]==403
    assert request('/api/pay/rollypay/status?id=rcd-other',headers=headers)[0]==404
    for change in [{'amount':'298.00'},{'currency':'USD'},{'payment_id':'other'},{'event_type':'payment.canceled'}]:assert event(**change)[0]==400
    assert event(test=True)[0]==200
    with sqlite3.connect(db) as c:assert c.execute('SELECT count(*) FROM recode_keys').fetchone()[0]==0
    assert event('expired')[0]==200
    assert event()[0]==200
    assert event()[0]==200
    assert event('created')[0]==200
    with sqlite3.connect(db) as c:
        assert c.execute('SELECT count(*) FROM payments').fetchone()[0]==1
        assert c.execute('SELECT count(*) FROM recode_keys WHERE revoked IS NULL').fetchone()[0]==1
        assert c.execute("SELECT state FROM rollypay_orders WHERE id='rcd-owned'").fetchone()[0]=='paid'
    assert event('refunded')[0]==200
    assert event()[0]==200
    assert event('refunded',event_type='refund_request.completed')[0]==200
    with sqlite3.connect(db) as c:
        assert c.execute('SELECT count(*) FROM recode_keys WHERE revoked IS NULL').fetchone()[0]==0
        assert c.execute("SELECT state FROM rollypay_orders WHERE id='rcd-owned'").fetchone()[0]=='refunded'
        c.execute("UPDATE rollypay_orders SET last_check=?",(int(time.time()),))
    code,h,b=request('/api/pay/rollypay/status?id=rcd-owned',headers=headers)
    assert code==200 and json.loads(b)=={'paid':False,'status':'refunded'},(code,b)
    print('PASS: redirect GET/POST; auth/origin/ownership; consent/prices/method; signatures/body limit; amount/currency/id/event checks; sandbox isolation; late paid/replay/refund; one entitlement and ledger record; no real DB touched')
finally:
    proc.terminate()
    try:proc.wait(timeout=10)
    except subprocess.TimeoutExpired:proc.kill();proc.wait()
    log.close()
