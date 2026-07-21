#include "dg_capi.h"

#include <stddef.h>
#include <stdint.h>
#include <string.h>

static DgStringView string_view(const char *text) {
  DgStringView view;
  view.data = text;
  view.len = text ? strlen(text) : 0;
  return view;
}

int main(void) {
  const char *options = "{\"shape\":[1,4],\"echo_inputs\":true}";
  struct DgBackend *backend = NULL;
  struct DgTensor *input = NULL;
  struct DgTensor *output = NULL;
  struct DgOwnedBytes *output_owned = NULL;
  size_t shape[] = {1, 4};
  float values[] = {1.0f, 2.0f, 3.0f, 4.0f};
  size_t input_count = 0;
  size_t output_count = 0;
  struct DgBackendCapabilities capabilities;
  DgByteView empty_model = {NULL, 0};
  DgByteView input_bytes;
  DgShapeView shape_view;

  input_bytes.data = (const uint8_t *)values;
  input_bytes.len = sizeof(values);
  shape_view.dims = shape;
  shape_view.rank = 2;

  if (dg_backend_create(Mock, empty_model, string_view(options), &backend,
                        NULL) != Ok ||
      dg_backend_io_counts(backend, &input_count, &output_count, NULL) != Ok ||
      dg_backend_capabilities(backend, &capabilities, NULL) != Ok ||
      dg_tensor_create(input_bytes, shape_view, F32, Nc, Cpu, &input, NULL) !=
          Ok) {
    dg_backend_free(backend);
    dg_tensor_free(input);
    return 1;
  }

  const struct DgTensor *inputs[] = {input};
  if (dg_backend_run(backend, inputs, 1, &output, 1, &output_count, NULL) !=
          Ok ||
      dg_tensor_data(output, &output_owned, NULL) != Ok) {
    dg_tensor_free(input);
    dg_tensor_free(output);
    dg_backend_free(backend);
    return 1;
  }

  (void)dg_owned_bytes_data(output_owned);
  (void)dg_owned_bytes_len(output_owned);
  dg_owned_bytes_free(output_owned);
  dg_tensor_free(output);
  dg_tensor_free(input);
  dg_backend_free(backend);
  return 0;
}
