#version 300 es
precision mediump float;

%% INCLUDE_COMMON_MATH %%
%% INCLUDE_COMMON_CAMERA %%
layout(location=0) in vec3 a_vertex;
layout(location=1) in vec3 a_normal;
layout(location=2) in vec3 a_color;

uniform mat4 u_model;

out vec3 v_vertex;
out vec3 v_normal;
flat out vec3 v_color;

void main() {
    mat4 mvp = (camera.projection * (camera.view * u_model));

    gl_Position = mvp * vec4(a_vertex,1);

    v_normal = a_normal;
    v_color = a_color;
    v_vertex = a_vertex;
}