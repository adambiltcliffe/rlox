import glob
import subprocess
import re
from colorama import Fore, Style, init

init()

binary = "./target/debug/rlox.exe"

expectedOutputPattern = re.compile(r"// expect: ?(.*)")
expectedErrorPattern = re.compile(r"// (Error.*)")
errorLinePattern = re.compile(r"// \[((java|c) )?line (\d+)\] (Error.*)")
expectedRuntimeErrorPattern = re.compile(r"// expect runtime error: (.+)")
syntaxErrorPattern = re.compile(r"\[.*line (\d+)\] (Error.+)")
stackTracePattern = re.compile(r"\[line (\d+)\]")
nonTestPattern = re.compile(r"// nontest")

skip = [
    "test\\benchmark",
    "test\\scanning",
    "test\\expressions",
    # functions
    "test\\call",
    "test\\closure",
    "test\\for\\closure_in_body.lox",
    "test\\for\\return_closure.lox",
    "test\\for\\return_inside.lox",
    "test\\for\\syntax.lox",
    "test\\function",
    "test\\limit\\no_reuse_constants.lox",
    "test\\limit\\stack_overflow.lox",
    "test\\limit\\too_many_constants.lox",
    "test\\limit\\too_many_locals.lox",
    "test\\limit\\too_many_upvalues.lox",
    "test\\regression\\40.lox",
    "test\\return",
    "test\\unexpected_character.lox",
    "test\\variable\\collide_with_parameter.lox",
    "test\\variable\\duplicate_parameter.lox",
    "test\\variable\\early_bound.lox",
    "test\\while\\closure_in_body.lox",
    "test\\while\\return_closure.lox",
    "test\\while\\return_inside.lox",
    # classes
    "test\\assignment\\to_this.lox",
    "test\\call\\object.lox",
    "test\\class",
    "test\\closure\\close_over_method_parameter.lox",
    "test\\constructor",
    "test\\field",
    "test\\inheritance",
    "test\\method",
    "test\\number\\decimal_point_at_eof.lox",
    "test\\number\\trailing_dot.lox",
    "test\\operator\\equals_class.lox",
    "test\\operator\\equals_method.lox",
    "test\\operator\\not.lox",
    "test\\operator\\not_class.lox",
    "test\\regression\\394.lox",
    "test\\return\\in_method.lox",
    "test\\super",
    "test\\this",
    "test\\variable\\local_from_method.lox",
    # inheritance
    "test\\class\\local_inherit_other.lox",
    "test\\class\\local_inherit_self.lox",
    "test\\class\\inherit_self.lox",
    "test\\class\\inherited_method.lox",
    "test\\inheritance",
    "test\\regression\\394.lox",
    "test\\super",
]


def should_test(filename):
    for s in skip:
        if filename.startswith(s):
            return False
    return True


def test_file(filename):
    print(f"{Fore.WHITE}===== {filename}")
    expected_output = []
    expected_errors = []
    expected_exit_code = 0
    expected_runtime_error = None
    with open(filename, encoding="utf-8") as f:
        for n, line in enumerate(f):
            r = expectedOutputPattern.search(line)
            if r:
                expected_output.append(r.groups(1)[0])
            r = expectedErrorPattern.search(line)
            if r:
                expected_errors.append(f"[line {n+1}] {r.groups(1)[0]}")
                expected_exit_code = 65
            r = errorLinePattern.search(line)
            if r:
                if r.groups()[1] is None or r.groups()[1] == "c":
                    expected_errors.append(
                        f"[line {r.groups(1)[2]}] {r.groups(1)[3]}")
                    expected_exit_code = 65
            r = expectedRuntimeErrorPattern.search(line)
            if r:
                expected_runtime_error = r.groups(1)[0]
                runtime_error_line = n
                expected_exit_code = 70
    result = subprocess.run(
        [binary, filename], capture_output=True, text=True, encoding="utf-8")
    ok = True
    if expected_runtime_error is not None:
        error_lines = result.stderr.split("\n")
        if len(error_lines) < 2:
            print(
                f"{Fore.RED} Expected runtime error '{expected_runtime_error}' but got none.")
            ok = False
        elif not error_lines[0].endswith(expected_runtime_error):
            print(
                f"{Fore.RED} Expected runtime error '{expected_runtime_error}' but got '{error_lines[0]}'.")
            ok = False
        # update this when we have proper stack traces!
    else:
        exerr = "\n".join(expected_errors)
        if exerr != result.stderr.rstrip():
            print(f"{Fore.RED} Expected error output was:{Fore.WHITE}\n{exerr}")
            print(
                f"{Fore.RED} But actual error output was:{Fore.WHITE}\n{result.stderr.rstrip()}")
            ok = False
    if result.returncode != expected_exit_code:
        print(
            f"{Fore.RED}Expected exit code {expected_exit_code} but got {result.returncode}")
        ok = False
    exout = "\n".join(expected_output)
    if exout != result.stdout.rstrip():
        print(f"{Fore.RED} Expected output was:{Fore.WHITE}\n{exout}")
        print(
            f"{Fore.RED} But actual output was:{Fore.WHITE}\n{result.stdout.rstrip()}")
        ok = False
    return ok


passes = 0
fails = 0

for fn in glob.glob("test/**/*.lox", recursive=True):
    if should_test(fn):
        if test_file(fn):
            passes += 1
        else:
            fails += 1

print(f"{passes} tests passed, {fails} tests failed.")
