#version 300 es
precision mediump float;

%% INCLUDE_COMMON_MATH %%
%% INCLUDE_COMMON_CAMERA %%

out vec4 color; 

void main() {
    color = vec4(1.0, 0.0, 0.0, 1.0);
}