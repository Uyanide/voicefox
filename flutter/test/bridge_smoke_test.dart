import "package:flutter_test/flutter_test.dart";
import "package:voicefox_flutter/src/bridge/frb_generated.dart";

void main() {
  test("generated Voicefox bridge is loadable", () {
    expect(RustLib.instance.codegenVersion, "2.13.0");
  });
}
