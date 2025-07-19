local Types = {}

local Vector = {}
export type Vector = {
    x: number,
    y: number,
    z: number
}

function Vector.create(x: number, y: number, z: number): Vector
    return {
        x = x,
        y = y,
        z = z
    }
end

function Types.create_plugin_data(public_name: string, author: string, internal_name: string, version: string)
    return {
        public_name = public_name,
        author = author,
        internal_name = internal_name,
        version = version
    }
end

local Items = {}

Items.Types = {
    WATERMELON = 45
}

function Items.create(type: number, pos: Vector): number
    _.Items.create(type, pos)
end

Types["Vector"] = Vector
Types["Items"] = Items
return Types