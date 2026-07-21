#include "dg_capi.h"

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static DgStringView string_view(const char *text) {
  DgStringView view;
  view.data = text;
  view.len = text ? strlen(text) : 0;
  return view;
}

static void print_error(struct DgError *error) {
  if (error) {
    const char *message = dg_error_message(error);
    if (message) {
      fprintf(stderr, "error: %s\n", message);
    }
    dg_error_free(error);
  }
}

static void cleanup_tensors(struct DgTensor *output,
                            struct DgTensor *input,
                            struct DgOwnedBytes *owned) {
  if (owned) {
    dg_owned_bytes_free(owned);
  }
  if (output) {
    dg_tensor_free(output);
  }
  if (input) {
    dg_tensor_free(input);
  }
}

int main(void) {
  struct DgEngine *engine = NULL;
  struct DgError *error = NULL;
  const char *spec =
      "apiVersion: dg/v1\n"
      "kind: Graph\n"
      "nodes:\n"
      "  - name: input\n"
      "    kind: input\n"
      "    params: {}\n"
      "  - name: infer\n"
      "    kind: mock_inference\n"
      "    params: {shape: [1, 4], echo_inputs: true}\n"
      "  - name: sink\n"
      "    kind: sink\n"
      "    params: {}\n"
      "connections:\n"
      "  - input.out -> infer.in\n"
      "  - infer.out -> sink.in\n";
  size_t shape[] = {1, 4};
  float input[] = {1.0f, 2.0f, 3.0f, 4.0f};
  struct DgTensor *tensor = NULL;
  struct DgTensor *output = NULL;
  struct DgOwnedBytes *output_owned = NULL;
  size_t added_nodes = 0;
  size_t removed_nodes = 0;
  size_t updated_nodes = 0;
  size_t added_connections = 0;
  size_t removed_connections = 0;
  DgByteView input_bytes;
  DgShapeView shape_view;

  input_bytes.data = (const uint8_t *)input;
  input_bytes.len = sizeof(input);
  shape_view.dims = shape;
  shape_view.rank = 2;

  if (dg_engine_create(&engine, &error) != Ok) {
    print_error(error);
    return 1;
  }

  if (dg_engine_load_string(engine, Yaml, string_view(spec), &error) != Ok ||
      dg_engine_diff_string(engine, Yaml, string_view(spec), &added_nodes,
                            &removed_nodes, &updated_nodes, &added_connections,
                            &removed_connections, &error) != Ok ||
      dg_engine_build(engine, &error) != Ok ||
      dg_tensor_create(input_bytes, shape_view, F32, Nc, Cpu, &tensor,
                       &error) != Ok ||
      dg_engine_push(engine, tensor, &error) != Ok ||
      dg_engine_run(engine, &error) != Ok ||
      dg_engine_poll(engine, &output, &error) != Ok ||
      dg_tensor_data(output, &output_owned, &error) != Ok) {
    print_error(error);
    cleanup_tensors(output, tensor, output_owned);
    dg_engine_destroy(engine, 0, NULL);
    return 1;
  }

  const uint8_t *output_data = dg_owned_bytes_data(output_owned);
  const size_t output_length = dg_owned_bytes_len(output_owned);
  (void)output_data;
  (void)output_length;

  cleanup_tensors(output, tensor, output_owned);

  if (dg_engine_destroy(engine, 5000, &error) != Ok) {
    print_error(error);
    return 1;
  }
  return 0;
}
