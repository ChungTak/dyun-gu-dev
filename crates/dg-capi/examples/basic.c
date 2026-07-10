#include "dg_capi.h"

#include <stddef.h>
#include <stdint.h>

int main(void) {
  struct DgEngine *engine = NULL;
  const char *spec =
      "apiVersion: dg/v1\n"
      "kind: Graph\n"
      "nodes:\n"
      "  - name: source\n"
      "    kind: source\n"
      "    params: {count: 0}\n"
      "  - name: infer\n"
      "    kind: mock_inference\n"
      "    params: {shape: [1, 4], echo_inputs: true}\n"
      "  - name: sink\n"
      "    kind: sink\n"
      "connections:\n"
      "  - source.out -> infer.in\n"
      "  - infer.out -> sink.in\n";
  size_t shape[] = {1, 4};
  uint8_t input[] = {1, 2, 3, 4};
  struct DgTensor *tensor = NULL;
  struct DgTensor *output = NULL;
  const uint8_t *output_data = NULL;
  size_t output_length = 0;

  if (dg_engine_create(&engine) != Ok ||
      dg_engine_load_string(engine, Yaml, spec) != Ok ||
      dg_engine_build(engine) != Ok ||
      dg_tensor_create(input, sizeof(input), shape, 2, U8, Nc, Cpu, &tensor) != Ok ||
      dg_engine_push(engine, tensor) != Ok ||
      dg_engine_poll(engine, &output) != Ok ||
      dg_tensor_data(output, &output_data, &output_length) != Ok) {
    dg_engine_free(engine);
    dg_tensor_free(tensor);
    dg_tensor_free(output);
    return 1;
  }

  (void)output_data;
  (void)output_length;
  dg_tensor_free(output);
  dg_tensor_free(tensor);
  dg_engine_free(engine);
  return 0;
}
