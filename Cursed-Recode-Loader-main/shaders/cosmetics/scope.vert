#version 450
// The sniper's scope over the whole picture (native/src/cosmetics/render.rs): one triangle past its corners.

void main() {
    vec2 p = vec2(float((gl_VertexIndex << 1) & 2), float(gl_VertexIndex & 2));
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
