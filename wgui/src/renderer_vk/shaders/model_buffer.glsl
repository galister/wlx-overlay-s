layout(std140, set = MODEL_BUFFER_SET,
       binding = 0) readonly buffer ModelBuffer {
  mat4 models[];
}
model_buffer;