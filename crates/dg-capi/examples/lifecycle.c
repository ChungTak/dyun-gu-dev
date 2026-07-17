#include "dg_capi.h"

#include <stdio.h>
#include <string.h>

int main(void) {
  struct DgEngine *engine = NULL;
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

  if (dg_engine_create(&engine) != Ok ||
      dg_engine_load_string(engine, Yaml, spec) != Ok ||
      dg_engine_init(engine) != Ok) {
    printf("init failed: %s\n", dg_last_error());
    dg_engine_free(engine);
    return 1;
  }

  enum DgGraphStatus status;
  if (dg_engine_status(engine, &status) != Ok) {
    printf("status failed: %s\n", dg_last_error());
    dg_engine_free(engine);
    return 1;
  }

  const char *metrics = NULL;
  size_t metrics_length = 0;
  if (dg_engine_metrics(engine, &metrics, &metrics_length) != Ok) {
    printf("metrics failed: %s\n", dg_last_error());
  }

  if (dg_engine_stop(engine) != Ok ||
      dg_engine_shutdown(engine, 5000) != Ok) {
    printf("stop/shutdown failed: %s\n", dg_last_error());
    dg_engine_free(engine);
    return 1;
  }

  printf("abi_version=%s package_version=%s status=%d metrics_length=%zu\n",
         dg_abi_version(), dg_version(), (int)status, metrics_length);
  (void)metrics;
  dg_engine_free(engine);
  return 0;
}
