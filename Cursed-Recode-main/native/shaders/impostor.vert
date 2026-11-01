#version 450
// The far terrain's six faces (vk/impostor.rs) laid over the frame: for each face a quad over the part of it that holds
// anything (camera-relative corners far out on its plane; four zeros when it holds nothing), all at the depth of the
// nearest point the faces can hold, so pixels where something nearer is already drawn are never shaded (the fragment
// shader only ever moves a pixel farther: depth_less).

layout(std140, binding = 0) uniform Impostor {
    mat4 inverseViewProj;
    mat4 viewProj;
    mat4 faces[6];
    vec4 origins[6];
    vec4 screen;
    vec4 misc;
    vec4 corners[24];
};

const int QUAD[6] = int[](0, 1, 2, 0, 2, 3);

layout(location = 0) flat out int face;

void main() {
    face = gl_VertexIndex / 6;
    vec4 p = viewProj * vec4(corners[(gl_VertexIndex / 6) * 4 + QUAD[gl_VertexIndex % 6]].xyz, 1.0);
    gl_Position = vec4(p.xy, misc.z * p.w, p.w);
}
