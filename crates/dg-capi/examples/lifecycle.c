#include "dg_capi.h"

#include <stdio.h>
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

int main(void) {
  struct DgAbiVersion abi = {sizeof(abi), 0, 0, 0};
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

  if (dg_engine_create(&engine, &error) != Ok ||
      dg_engine_load_string(engine, Yaml, string_view(spec), &error) != Ok ||
      dg_engine_init(engine, &error) != Ok) {
    print_error(error);
    dg_engine_destroy(engine, 0, NULL);
    return 1;
  }

  enum DgGraphStatus status;
  struct DgOwnedBytes *cause = NULL;
  if (dg_engine_status(engine, &status, NULL, &error) != Ok) {
    print_error(error);
    dg_engine_destroy(engine, 5000, NULL);
    return 1;
  }

  struct DgOwnedBytes *metrics = NULL;
  if (dg_engine_metrics(engine, &metrics, &error) != Ok) {
    print_error(error);
    dg_engine_destroy(engine, 5000, NULL);
    return 1;
  }

  if (dg_engine_stop(engine, &error) != Ok ||
      dg_engine_shutdown(engine, 5000, &error) != Ok) {
    print_error(error);
    if (metrics) {
      dg_owned_bytes_free(metrics);
    }
    dg_engine_destroy(engine, 5000, NULL);
    return 1;
  }

  if (dg_abi_version(&abi, &error) != Ok) {
    print_error(error);
  }

  printf("abi_version=%u.%u package_version=%s status=%d metrics_length=%zu\n",
         (unsigned)abi.major, (unsigned)abi.minor, dg_version(), (int)status,
         dg_owned_bytes_len(metrics));
  if (cause) {
    dg_owned_bytes_free(cause);
  }
  if (metrics) {
    dg_owned_bytes_free(metrics);
  }

  if (dg_engine_destroy(engine, 5000, &error) != Ok) {
    print_error(error);
    return 1;
  }
  return 0;
}
