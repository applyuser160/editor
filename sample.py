from pydantic import BaseModel

class MyModel(BaseModel):
    name: str

def main():
    my_model = MyModel(name="sample")
    print(my_model)

main()