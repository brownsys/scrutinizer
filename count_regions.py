import json
import sys

truly_pure = 0
truly_impure = 0
pure_unrecognized = 0
impure_unrecognized = 0

with open(sys.argv[1]) as file:
    input = json.loads(file.read())
    total_bodies = 0
    for region in input['results']:
        total_bodies += len(region['failing'])
        total_bodies += len(region['passing'])
        if region['annotated_pure'] == True and region['status'] == True:
            truly_pure += 1
        elif region['annotated_pure'] == False and region['status'] == False:
            truly_impure += 1
        elif region['annotated_pure'] == True and region['status'] == False:
            print(region['def_id'])
            pure_unrecognized += 1
        elif region['annotated_pure'] == False and region['status'] == True:
            impure_unrecognized += 1

    print(f"Crate analyzed: {input['crate_name']}")
    print("-----")
    print(f"Pure / Determined pure:\t\t{truly_pure}")
    print(f"Impure / Determined impure:\t{truly_impure}")
    print(f"Pure / Determined impure:\t{pure_unrecognized}")
    print(f"Impure / Determined pure:\t{impure_unrecognized}")
    print(f"Bodies analyzed:\t\t{total_bodies}")
    print(f"Time taken (s):\t\t\t{input['elapsed']}")
