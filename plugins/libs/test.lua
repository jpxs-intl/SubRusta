local types = require("libs/subrustatypes")

plugin_info = types.create_plugin_data("Demo Plugin", "Infinity Dev", "demo_plugin", "v1.0.0")

local item_type = -1
function tick()
    if item_type == -1 then
        print("Making item!")
        
        item_type = types.Items.create(types.Items.Types.WATERMELON, types.Vector.create(1805.0, 87.0, 1530.0))
    end
end