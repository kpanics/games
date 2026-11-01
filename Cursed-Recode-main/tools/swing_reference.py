"""Regenerate the source comparison fixture from locally supplied source trees.
Run this script with NEWCODE CATLAVAN NURIK OUTPUT, javac OUTPUT/SwingSourceReference.java,
then java -cp OUTPUT SwingSourceReference > native/test-data/swing-source-reference.json.
The harness executes extracted Java expressions; no game, network or native client loads.
Nurik's restored symbols use the axis mapping of its vanilla swing handler (u=Y+,R=Z+,y=X+,N=X-).
This validates the provided source bodies, not equivalence to unavailable original binaries.
"""
from pathlib import Path
import re,json,hashlib
import argparse
parser=argparse.ArgumentParser(description="Extract supplied Java swing formulas into an isolated reference harness; does not run Minecraft.")
parser.add_argument("newcode",type=Path)
parser.add_argument("catlavan",type=Path)
parser.add_argument("nurik",type=Path)
parser.add_argument("output",type=Path)
args=parser.parse_args()
w=args.output
w.mkdir(parents=True,exist_ok=True)
new=(args.newcode/'src/net/minecraft/client/renderer/FirstPersonRenderer.java').read_text(encoding='utf-8',errors='replace')
cat=(args.catlavan/'src/main/java/net/minecraft/client/renderer/FirstPersonRenderer.java').read_text(encoding='utf-8')
catmod=(args.catlavan/'src/main/java/catlavan/module/visuals/SwingAnimationsModule.java').read_text(encoding='utf-8')
def body(s,mark):
 a=s.index('{',s.index(mark));depth=1;b=a+1
 while depth:
  depth+=(s[b]=='{')-(s[b]=='}');b+=1
 return s[a+1:b-1]
nb=body(new,'if (animationFunction.enabled && handside == HandSide.RIGHT)').replace('System.currentTimeMillis()','clock')
cb=body(cat,'switch(s)')
cn=body(catmod,'public boolean applyHandTransform(')
ce=body(catmod,'private float backEasingInOut(')
header=r"""import java.util.*;
public class SwingSourceReference {
 static long clock; static float progress; static int kind; boolean swingPhaseFlipped;
 static class MathHelper {
  static float[] S=new float[65536]; static{for(int i=0;i<S.length;i++)S[i]=(float)Math.sin(i*Math.PI*2.0/65536.0);}
  static float sin(float x){return S[(int)(x*10430.378F)&65535];}
  static float sqrt(float x){return (float)Math.sqrt(x);}
  static float clamp(float x,float a,float b){return Math.min(b,Math.max(a,x));}
 }
 record Rot(float x,float y,float z,float a){}
 static class Vector3f {
  static Vector3f XP=new Vector3f(1,0,0),XN=new Vector3f(-1,0,0),YP=new Vector3f(0,1,0),ZP=new Vector3f(0,0,1);
  float x,y,z;Vector3f(float a,float b,float c){x=a;y=b;z=c;}
  Rot rotationDegrees(float a){return new Rot(x,y,z,a);}
 }
 static class RotationUtility {static Rot fromDegrees(float x,float y,float z,float a){return new Rot(x,y,z,a);}}
 static class MatrixStack {
  List<float[]> ops=new ArrayList<>();
  void translate(double x,double y,double z){if(x!=0||y!=0||z!=0)ops.add(new float[]{0,(float)x,(float)y,(float)z});}
  void rotate(Rot r){if(r.a()!=0)ops.add(new float[]{1,r.x(),r.y(),r.z(),r.a()});}
  void scale(float x,float y,float z){if(x!=1||y!=1||z!=1){if(x!=y||y!=z)throw new AssertionError();ops.add(new float[]{2,x});}}
 }
 static class F {float v;Float get(){return v;}Float getValue(){return v;}}
 static class Mode {String name;boolean is(String s){return name.equals(s);}}
 enum HandSide{RIGHT,LEFT} enum Hand{MAIN_HAND} enum UseAction{NONE,SPEAR,BLOCK}
 static class Item{} static class ToolItem extends Item{} static class AxeItem extends ToolItem{} static class ShovelItem extends ToolItem{} static class SwordItem extends Item{}
 static class ItemStack {boolean isEmpty(){return false;} Item getItem(){return switch(kind){case 0->new SwordItem();case 1->new AxeItem();case 2->new ShovelItem();case 3->new ToolItem();default->new Item();};}UseAction getUseAction(){return kind==4?UseAction.SPEAR:kind==5?UseAction.BLOCK:UseAction.NONE;}}
 static class AbstractClientPlayerEntity{float swingProgress;boolean isSwingInProgress;float getSwingProgress(float p){return progress;}ItemStack getHeldItem(Hand h){return new ItemStack();}}
 static class MC{AbstractClientPlayerEntity player=new AbstractClientPlayerEntity();}MC mc=new MC();
 class Config {
  F angle=new F(),swipePower=new F(),angleSlider=angle,swipePowerSlider=swipePower;
  Mode swordAnim=new Mode();
  boolean applyHandTransform(MatrixStack m,AbstractClientPlayerEntity p,Hand h,float a,boolean l){return catNew(m,p,h,a,l);}
 }
 void updateSwingPhase(float p){}
 float prevEquippedProgressMainHand,equippedProgressMainHand;
 void transformFirstPerson(MatrixStack m,HandSide hand,float p){
  int d=hand==HandSide.RIGHT?1:-1;float a=MathHelper.sin(p*p*(float)Math.PI),b=MathHelper.sin(MathHelper.sqrt(p)*(float)Math.PI);
  m.rotate(Vector3f.YP.rotationDegrees(d*(45+a*-20)));m.rotate(Vector3f.ZP.rotationDegrees(d*b*-20));m.rotate(Vector3f.XP.rotationDegrees(b*-80));m.rotate(Vector3f.YP.rotationDegrees(d*-45));
 }
"""
s=header+'boolean catNew(MatrixStack var1,AbstractClientPlayerEntity var2,Hand var3,float var4,boolean var5){'+cn+'}\n'
s+='float backEasingInOut(float var1){'+ce+'}\n'
s+='void nc(Config animationFunction,MatrixStack matrixStackIn,float swingProgress,int l,float equippedProgress){float partialTicks=1;boolean flag3=l==1;HandSide handside=flag3?HandSide.RIGHT:HandSide.LEFT;float anim=(float)Math.sin(swingProgress*(Math.PI/2)*2);'+nb+'}\n'
s+='void cat(Config swinganimationsmodule,String s,MatrixStack matrixStackIn,float swingProgress,int d){boolean flag3=d<0;AbstractClientPlayerEntity player=mc.player;Hand handIn=Hand.MAIN_HAND;HandSide handside=flag3?HandSide.LEFT:HandSide.RIGHT;float f3=(float)Math.sin((double)swingProgress*2.0943951023931953D*1.5D);switch(s){'+cb+'}}\n'
s+=r"""
 public static void main(String[] args){
  String[][] names={ {},{}, {"Mode 1","Mode 2","Mode 3","Mode 4","Mode 5","Mode 6","Mode 7","360","Fade","Slant","New"}, {"None","Self","Block","Block Old","Smooth","Rotate","Break","Slap","Push"}};
  System.out.print("[");boolean comma=false;
  for(int source=2;source<=3;source++)for(int mode=0;mode<names[source].length;mode++)for(int point=0;point<=12;point++)for(int d:new int[]{-1,1})for(int variant=0;variant<3;variant++){
   if(source==3&&d<0)continue;
   for(int item=0;item<(source==2&&mode==10?7:1);item++){
    SwingSourceReference r=new SwingSourceReference();Config cfg=r.new Config();kind=item;progress=point/12f;
    cfg.angle.v=variant==0?100:variant==1?0:273;cfg.swipePower.v=variant==0?8:variant==1?1:10;cfg.swordAnim.name=names[source][mode];
    clock=variant==0?141:variant==1?0:254;boolean active=point>0&&point<12;
    r.mc.player.swingProgress=active?progress:0;r.mc.player.isSwingInProgress=active;r.swingPhaseFlipped=variant==2;
    float equip=variant*0.4f;r.prevEquippedProgressMainHand=r.equippedProgressMainHand=1-equip;
    MatrixStack matrix=new MatrixStack();if(source==2)r.cat(cfg,names[source][mode],matrix,progress,d);else r.nc(cfg,matrix,progress,d,equip);
    if(comma)System.out.print(",");comma=true;
    System.out.print("{\"source\":"+source+",\"mode\":"+mode+",\"p\":"+progress+",\"d\":"+d+",\"angle\":"+cfg.angle.v+",\"power\":"+cfg.swipePower.v+",\"equip\":"+equip+",\"time\":"+clock+",\"active\":"+active+",\"flipped\":"+r.swingPhaseFlipped+",\"kind\":"+item+",\"ops\":[");
    for(int i=0;i<matrix.ops.size();i++){if(i>0)System.out.print(",");System.out.print(Arrays.toString(matrix.ops.get(i)));}
    System.out.print("]}");
   }
  }
  System.out.print("]");
 }
}
"""
(w/'SwingSourceReference.java').write_text(s,encoding='utf-8')
print('Reference harness contains original Java method/switch bodies, not a reimplementation')

# Add Nurik's four restored handler bodies without approximating their curves.
np=(args.nurik/'rw/core')
variants=[('SwingAnimationVariantHandler.java','public void applyAnimation(', 'float p3,float p4,float p5,float progress,float p7'),('IlsIskr.java','public void apply(', 'float unused1,float swingProgress,float unused2,float swing,float unused3'),('SwingAnimationStaticPoseHandler.java','public void applyAnimation(', 'float p3,float p4,float p5,float p6,float p7'),('SwingAnimationMultiAxisHandler.java','public void applyAnimation(', 'float p3,float p4,float p5,float progress,float p7')]
extra='static boolean spinning; static float timeTicks;\n'
extra+='static class NNWE{static float m(float v){return MathHelper.sin(v);}static float N(float v){return MathHelper.sqrt(v);}}\n'
extra+='static class NNNJZ {static Vector3f u=Vector3f.YP,R=Vector3f.ZP,y=Vector3f.XP,N=Vector3f.XN;}\n'
for i,(file,mark,args) in enumerate(variants):
 b=body((np/file).read_text(encoding='utf-8'),mark)
 b=b.replace('(Boolean)((SwingAnimations)this.N[0]).m12000().i()','spinning')
 b=b.replace('NursultanClient.m74000().m10000() + NNuU.Nq().NK().N(true)','timeTicks')
 extra+='void nur'+str(i)+'(MatrixStack '+('matrixStack' if i==1 else 'pose')+',int '+('equipProgress' if i==1 else 'direction')+','+args+'){'+b+'}\n'
s=s.replace('Rot rotationDegrees(float a){','Rot N(float a){return rotationDegrees(a);} Rot rotationDegrees(float a){')
s=s.replace('void translate(double x,double y,double z){','void N(float x,float y,float z){translate(x,y,z);} void N(Rot r){rotate(r);} void translate(double x,double y,double z){')
s=s.replace(' public static void main(String[] args){',extra+' public static void main(String[] args){')
s=s.replace('String[][] names={ {},{},','String[][] names={ {},{"Swing 1","Swing 2","Swing 3","Swing 4"},')
s=s.replace('for(int source=2;source<=3;source++)','for(int source=1;source<=3;source++)')
s=s.replace('clock=variant==0?141:variant==1?0:254;', 'spinning=variant==2;timeTicks=123.5f;clock=variant==0?141:variant==1?0:254;')
s=s.replace('if(source==2)r.cat(', 'if(source==1){float power=cfg.swipePower.v*10, wave=MathHelper.sin(progress*(float)(Math.PI/2)*2),smooth=variant==0?8:variant==1?3:10;switch(mode){case 0->r.nur0(matrix,d,wave,smooth,power,progress,equip);case 1->r.nur1(matrix,d,wave,smooth,power,progress,equip);case 2->r.nur2(matrix,d,wave,smooth,power,progress,equip);case 3->r.nur3(matrix,d,wave,smooth,power,progress,equip);}}else if(source==2)r.cat(')
s=s.replace('System.out.print("{\\"source\\":"+source', 'System.out.print("{\\"spinning\\":"+spinning+",\\"time_ticks\\":"+timeTicks+",\\"smoothness\\":"+(variant==0?8:variant==1?3:10)+",\\"source\\":"+source')
(w/'SwingSourceReference.java').write_text(s,encoding='utf-8')
