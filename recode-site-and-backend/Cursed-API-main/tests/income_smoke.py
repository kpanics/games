"""Run against an isolated database only; no real provider credentials or transactions."""
import hashlib,json,os,pathlib,sqlite3,subprocess,time,urllib.request,urllib.error,tempfile,sys
base=pathlib.Path(os.environ.get('INCOME_TEST_DIR','/root/build/income-20261027'));root=pathlib.Path(tempfile.mkdtemp(prefix='income-http-',dir=base));database=root/'test.db';port=18087
at=int(time.time());origin=f'http://127.0.0.1:{port}'
env=os.environ.copy();env.update(CURSED_DB=str(database),CURSED_LISTEN=f'127.0.0.1:{port}',CURSED_ORIGIN=origin,CURSED_SECURE_COOKIE='0',CURSED_MEDIA=str(root/'media'),CURSED_DIST=str(root/'dist'),ROLLYPAY_ENABLED='1',ROLLYPAY_API_KEY='unused-test',ROLLYPAY_TERMINAL_ID='unused-test',ROLLYPAY_SIGNING_SECRET='isolated-test-secret')
log=open(root/'api.log','wb');proc=None

def call(path,uid=None,body=None,origin_override=None):
 headers={'Origin':origin_override or origin}
 if uid is not None:headers['Cookie']='cursed_session='+hashlib.sha256(str(uid).encode()).hexdigest()
 if body is not None:body=json.dumps(body).encode();headers['Content-Type']='application/json'
 req=urllib.request.Request(origin+path,data=body,headers=headers)
 try:r=urllib.request.urlopen(req,timeout=20)
 except urllib.error.HTTPError as e:r=e
 data=r.read()
 try:data=json.loads(data)
 except ValueError:pass
 return r.status,data

def start():
 global proc
 proc=subprocess.Popen([os.environ['INCOME_TEST_BINARY'],'serve'],env=env,stdout=log,stderr=log)
 for _ in range(80):
  try:
   if call('/api/pay/rollypay/callback')[0]==200:return
  except OSError:pass
  if proc.poll() is not None:raise AssertionError('startup failed '+str(root))
  time.sleep(.15)
 raise AssertionError('startup timeout')

def stop():
 if proc and proc.poll() is None:
  proc.terminate()
  try:proc.wait(timeout=10)
  except subprocess.TimeoutExpired:proc.kill();proc.wait()
try:
 start();stop()
 with sqlite3.connect(database) as c:
  for uid,name in [(0,'Lunuca'),(622,'Janade'),(19,'Andrey_AKM'),(42,'Media_Test'),(77,'Buyer_Test')]:
   c.execute('INSERT OR IGNORE INTO users(uid,login,login_key,email,email_key,pass,created) VALUES(?,?,?,?,?,?,?)',(uid,name,name.lower(),f'{uid}@example.invalid',f'{uid}@example.invalid','unused',at))
   token=hashlib.sha256(str(uid).encode()).hexdigest();c.execute("INSERT INTO sessions(hash,uid,kind,created,seen) VALUES(?,?,'web',?,?)",(hashlib.sha256(token.encode()).digest(),uid,at,at))
  c.execute("INSERT INTO roles(uid,role,since) VALUES(42,'media',?)",(at,))
 start()
 assert call('/api/income')[0]==401
 assert call('/api/income/access',77)==(200,{'allowed':False})
 for uid in [0,622,19,42]:assert call('/api/income/access',uid)==(200,{'allowed':True})
 assert call('/api/income',77)[0]==403
 assert call('/api/income',0)[1]['summary']['amount']==0
 assert call('/api/pay/quote?days=30&promo=recode',77)[1]['amount']==26910
 assert call('/api/pay/quote?days=90&promo=RECODE',77)[1]['amount']==44910
 assert call('/api/pay/quote?days=0&promo=RECODE',77)[1]['amount']==62910
 assert call('/api/pay/quote?days=30&promo=unknown',77)[0]==400
 assert call('/api/income/code',77,{'code':'HACK'})[0]==403
 assert call('/api/income/code',42,{'code':'FRAME'})[0]==200
 d=call('/api/income',42)[1];codeid=d['codes'][0]['id']
 assert d['codes'][0]['discount']==1500 and d['codes'][0]['share']==1500
 assert call('/api/pay/quote?days=30&promo=FRAME',77)[1]['amount']==25415
 assert call('/api/income/code',42,{'id':codeid,'code':'FRAME','share':10000})[0]==403
 assert call('/api/income/code',77,{'id':codeid,'code':'STOLEN'})[0]==403
 assert call('/api/income/code',42,{'id':codeid,'code':'RENAMED'})[0]==200
 assert call('/api/pay/quote?days=30&promo=FRAME',77)[0]==400
 assert call('/api/income/code',0,{'code':'FRAME','uid':77})[0]==400
 assert call('/api/income/rates',622,{'admin':0,'admin2':0,'media':0})[0]==403
 assert call('/api/income/rates',0,{'admin':9000,'admin2':9000,'media':1500})[0]==400
 assert call('/api/income/rates',0,{'admin':2000,'admin2':500,'media':1500},'https://evil.invalid')[0]==403
 assert call('/api/income/payout',0,{'id':'a'*32,'uid':42,'amount':1})[0]==400
 assert call('/api/income/payout',42,{'id':'a'*32,'uid':42,'amount':1})[0]==403
 assert call('/api/pay/rollypay/create',77,{'days':30,'method':'sbp','agree':True,'promo':'RECODE','expected_amount':29900})[0]==409
 with sqlite3.connect(database) as c:assert c.execute('SELECT count(*) FROM income_orders').fetchone()[0]==0
 # Fixture simulates a confirmed purchase; calculations and atomic payment lifecycle are tested in Rust.
 with sqlite3.connect(database) as c:
  c.execute("INSERT INTO rollypay_orders(id,uid,days,amount,bill,made,state,fulfilled) VALUES('rcd-ui-fixture',77,30,25415,'ui-fixture',?,'paid',?)",(at,at))
  c.execute("INSERT INTO income_orders(id,list_price,amount,promo_id,promo,media_uid,discount,admin_rate,admin2_rate,media_rate,method,made,fee,fee_rate,net,settled) VALUES('rcd-ui-fixture',29900,25415,?,'FRAME',42,1500,2000,500,1500,'sbp',?,2287,900,23128,?)",(codeid,at,at))
  for uid,amount in [(0,13877),(622,4626),(19,1156),(42,3469)]:c.execute("INSERT INTO income_entries VALUES('rcd-ui-fixture',?,'earned',?,?)",(uid,amount,at))
 d=call('/api/income',0)[1];assert d['summary']==dict(mine=13877,count=1,amount=25415,fee=2287,admins=5782,media=3469),d
 d=call('/api/income',42)[1];assert d['summary']['mine']==3469 and d['summary']['amount']==0 and 'fee' not in d['orders'][0] and d['rates'] is None
 body={'id':'a'*32,'uid':42,'amount':1000,'note':'test transfer'}
 assert call('/api/income/payout',0,body)[0]==200
 assert call('/api/income/payout',0,body)[0]==200
 assert call('/api/income',42)[1]['available']==2469
 assert call('/api/income/payout',0,dict(body,amount=2000))[0]==400
 assert call('/api/income',19)[1]['summary']['mine']==1156
 print('PASS income HTTP: authentication, origin, recipient isolation, codes, rename/reserved names, rates, quotes for all terms, aggregate money, manual payout limits and idempotency. Isolated fixtures only.')
 if '--keep' in sys.argv:
  (base/'ui-test-process.json').write_text(json.dumps({'pid':proc.pid,'root':str(root),'db':str(database),'port':port}))
  print('UI staging running',proc.pid,root);proc=None
finally:stop();log.close()
